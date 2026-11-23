"""
Capa de red del cliente: un socket TCP y un socket UDP hacia el servidor.
Los mensajes recibidos se entregan a una función callback(evento: dict).
Licencia: MIT.
"""
import json
import socket
import threading
import time


class ChatClient:
    def __init__(self, host, tcp_port, udp_port, on_event):
        self.host = host
        self.tcp_port = tcp_port
        self.udp_addr = (host, udp_port)
        self.on_event = on_event
        self.name = None
        self.tcp = None
        self.udp = None
        self.running = False
        self.seq = 0
        self.udp_ok = False

    # --------------------------------------------------------------- conexión
    def connect(self, name, timeout=5):
        self.tcp = socket.create_connection((self.host, self.tcp_port), timeout)
        self.tcp.settimeout(timeout)
        self._tcp_send({"type": "login", "name": name})
        reply = self._read_one_line()
        if reply.get("type") != "welcome":
            self.tcp.close()
            raise ConnectionError(reply.get("text", "login rechazado"))
        self.name = reply["name"]
        self.tcp.settimeout(None)

        self.udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.udp.bind(("0.0.0.0", 0))
        self.running = True
        threading.Thread(target=self._tcp_loop, daemon=True).start()
        threading.Thread(target=self._udp_loop, daemon=True).start()
        threading.Thread(target=self._heartbeat, daemon=True).start()
        self._udp_send({"type": "hello", "name": self.name})
        self.on_event({"type": "welcome", "users": reply.get("users", [])})

    def _read_one_line(self):
        buf = b""
        while b"\n" not in buf:
            chunk = self.tcp.recv(4096)
            if not chunk:
                raise ConnectionError("el servidor cerró la conexión")
            buf += chunk
        line, self._pending = buf.split(b"\n", 1)
        return json.loads(line.decode())

    def close(self):
        if not self.running:
            return
        self.running = False
        try:
            self._tcp_send({"type": "quit"})
        except OSError:
            pass
        for s in (self.tcp, self.udp):
            try:
                s.close()
            except OSError:
                pass

    # ---------------------------------------------------------------- envío
    def _tcp_send(self, obj):
        self.tcp.sendall((json.dumps(obj, ensure_ascii=False) + "\n").encode())

    def _udp_send(self, obj):
        self.udp.sendto(json.dumps(obj, ensure_ascii=False).encode(),
                        self.udp_addr)

    def send(self, text, proto="TCP", to=None):
        if proto == "UDP":
            self.seq += 1
            msg = {"type": "msg", "name": self.name, "text": text,
                   "seq": self.seq, "ts": time.time()}
            if to:
                msg["to"] = to
            self._udp_send(msg)
        else:
            msg = {"type": "msg", "text": text}
            if to:
                msg["to"] = to
            self._tcp_send(msg)

    def request(self, kind):
        self._tcp_send({"type": kind})

    # -------------------------------------------------------------- recepción
    def _tcp_loop(self):
        buf = getattr(self, "_pending", b"")
        while self.running:
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                if line.strip():
                    try:
                        self.on_event(json.loads(line.decode()))
                    except ValueError:
                        pass
            try:
                chunk = self.tcp.recv(4096)
            except OSError:
                chunk = b""
            if not chunk:
                if self.running:
                    self.running = False
                    self.on_event({"type": "disconnected"})
                return
            buf += chunk

    def _udp_loop(self):
        while self.running:
            try:
                data, _ = self.udp.recvfrom(4096)
            except OSError:
                return
            try:
                msg = json.loads(data.decode())
            except ValueError:
                continue
            if msg.get("type") == "hello_ok":
                self.udp_ok = True
            if msg.get("type") == "pong":
                rtt = (time.time() - (msg.get("ts") or time.time())) * 1000
                msg["rtt_ms"] = round(rtt, 2)
            self.on_event(msg)

    def _heartbeat(self):
        n = 0
        while self.running:
            time.sleep(5)
            n += 1
            try:
                self._udp_send({"type": "ping", "name": self.name, "seq": n,
                                "ts": time.time()})
            except OSError:
                return
