#!/usr/bin/env python3
"""
Servidor de chat TCP + UDP (se ejecuta dentro de la VM "host principal").

- TCP (puerto 5000 por defecto): conexión fiable. Login, lista de usuarios,
  mensajes públicos y privados.
- UDP (puerto 5001 por defecto): datagramas sin conexión. Registro (HELLO),
  mensajes públicos/privados y latidos (PING). El servidor registra en el log
  cada paquete UDP recibido y lo reenvía a los demás usuarios.

Los mensajes enviados por TCP se reparten también a los usuarios por TCP, y
los enviados por UDP se reparten por UDP. Así se puede comparar el
comportamiento de ambos protocolos (orden, pérdidas, latencia).

Formato: JSON en UTF-8. En TCP, un objeto JSON por línea ("\n").
En UDP, un objeto JSON por datagrama.

Solo usa la biblioteca estándar de Python 3 (sin dependencias).
Licencia: MIT.
"""
import argparse
import json
import logging
import selectors
import socket
import time

MAX_UDP = 4096


class ChatServer:
    def __init__(self, host, tcp_port, udp_port, max_users):
        self.max_users = max_users
        self.sel = selectors.DefaultSelector()

        self.tcp = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.tcp.bind((host, tcp_port))
        self.tcp.listen()
        self.tcp.setblocking(False)
        self.sel.register(self.tcp, selectors.EVENT_READ, "tcp_accept")

        self.udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.udp.bind((host, udp_port))
        self.udp.setblocking(False)
        self.sel.register(self.udp, selectors.EVENT_READ, "udp")

        # nombre -> {"sock": socket|None, "buf": bytes, "addr": (ip,port),
        #            "udp": (ip,port)|None, "seen": float}
        self.users = {}
        self.sock_to_name = {}
        self.stats = {"tcp_in": 0, "udp_in": 0, "udp_out": 0, "tcp_out": 0}
        logging.info("Escuchando TCP %s:%d y UDP %s:%d (máx. %d usuarios)",
                     host, tcp_port, host, udp_port, max_users)

    # ------------------------------------------------------------ utilidades
    def user_list(self):
        return sorted(self.users)

    def tcp_send(self, sock, obj):
        try:
            sock.sendall((json.dumps(obj, ensure_ascii=False) + "\n").encode())
            self.stats["tcp_out"] += 1
        except OSError:
            pass

    def udp_send(self, addr, obj):
        try:
            self.udp.sendto(json.dumps(obj, ensure_ascii=False).encode(), addr)
            self.stats["udp_out"] += 1
        except OSError:
            pass

    def broadcast_tcp(self, obj, exclude=None):
        for name, u in self.users.items():
            if name != exclude and u["sock"]:
                self.tcp_send(u["sock"], obj)

    def broadcast_udp(self, obj, exclude=None):
        for name, u in self.users.items():
            if name != exclude and u["udp"]:
                self.udp_send(u["udp"], obj)

    def announce(self, text):
        msg = {"type": "system", "text": text, "users": self.user_list(),
               "ts": time.time()}
        self.broadcast_tcp(msg)

    # ------------------------------------------------------------------ TCP
    def accept(self):
        conn, addr = self.tcp.accept()
        conn.setblocking(False)
        self.sel.register(conn, selectors.EVENT_READ, "tcp_client")
        self.sock_to_name[conn] = None
        conn_buf[conn] = b""
        logging.info("TCP conexión nueva desde %s:%d", *addr)

    def close_tcp(self, sock):
        name = self.sock_to_name.pop(sock, None)
        conn_buf.pop(sock, None)
        try:
            self.sel.unregister(sock)
        except (KeyError, ValueError):
            pass
        sock.close()
        if name and name in self.users:
            del self.users[name]
            logging.info("Usuario '%s' desconectado", name)
            self.announce(f"{name} salió del chat")

    def read_tcp(self, sock):
        try:
            data = sock.recv(4096)
        except (ConnectionResetError, OSError):
            data = b""
        if not data:
            self.close_tcp(sock)
            return
        conn_buf[sock] += data
        while b"\n" in conn_buf.get(sock, b""):
            line, conn_buf[sock] = conn_buf[sock].split(b"\n", 1)
            if not line.strip():
                continue
            try:
                msg = json.loads(line.decode())
            except (ValueError, UnicodeDecodeError):
                self.tcp_send(sock, {"type": "error", "text": "JSON inválido"})
                continue
            self.stats["tcp_in"] += 1
            self.handle_tcp(sock, msg)
            if sock not in self.sock_to_name:  # se cerró dentro del handler
                return

    def handle_tcp(self, sock, msg):
        kind = msg.get("type")
        name = self.sock_to_name.get(sock)
        peer = sock.getpeername()

        if kind == "login":
            wanted = str(msg.get("name", "")).strip()[:20]
            if name:
                return
            if not wanted or wanted.lower() == "servidor":
                self.tcp_send(sock, {"type": "error", "text": "Nombre no válido"})
                return
            if wanted in self.users:
                self.tcp_send(sock, {"type": "error", "text": "Nombre en uso"})
                return
            if len(self.users) >= self.max_users:
                self.tcp_send(sock, {"type": "error",
                                     "text": f"Sala llena ({self.max_users})"})
                return
            self.users[wanted] = {"sock": sock, "addr": peer, "udp": None,
                                  "seen": time.time()}
            self.sock_to_name[sock] = wanted
            logging.info("TCP login '%s' desde %s:%d", wanted, *peer)
            self.tcp_send(sock, {"type": "welcome", "name": wanted,
                                 "users": self.user_list()})
            self.announce(f"{wanted} entró al chat")
            return

        if not name:
            self.tcp_send(sock, {"type": "error", "text": "Primero haz login"})
            return

        if kind == "msg":
            text = str(msg.get("text", ""))[:1000]
            to = msg.get("to")
            out = {"type": "msg", "from": name, "text": text, "proto": "TCP",
                   "ts": time.time()}
            logging.info("TCP %s -> %s: %s", name, to or "todos", text)
            if to:
                out["private"] = True
                out["to"] = to
                target = self.users.get(to)
                if target and target["sock"]:
                    self.tcp_send(target["sock"], out)
                    self.tcp_send(sock, out)  # eco al remitente
                else:
                    self.tcp_send(sock, {"type": "error",
                                         "text": f"'{to}' no está conectado"})
            else:
                self.broadcast_tcp(out)
        elif kind == "list":
            self.tcp_send(sock, {"type": "system", "text": "Usuarios conectados",
                                 "users": self.user_list()})
        elif kind == "stats":
            self.tcp_send(sock, {"type": "stats", **self.stats,
                                 "users": self.user_list()})
        elif kind == "quit":
            self.close_tcp(sock)

    # ------------------------------------------------------------------ UDP
    def read_udp(self):
        while True:
            try:
                data, addr = self.udp.recvfrom(MAX_UDP)
            except BlockingIOError:
                return
            except OSError:
                return
            self.stats["udp_in"] += 1
            try:
                msg = json.loads(data.decode())
            except (ValueError, UnicodeDecodeError):
                logging.warning("UDP %s:%d paquete no JSON (%d bytes)",
                                *addr, len(data))
                continue
            # Los ping llegan cada 3 s por usuario: solo se ven con --verbose.
            nivel = logging.DEBUG if msg.get("type") == "ping" else logging.INFO
            logging.log(nivel, "UDP recibido de %s:%d (%d bytes): %s",
                        *addr, len(data), msg)
            self.handle_udp(addr, msg)

    def handle_udp(self, addr, msg):
        kind = msg.get("type")
        name = msg.get("name")
        user = self.users.get(name)
        # El usuario debe haber hecho login por TCP desde la misma IP.
        if not user or user["addr"][0] != addr[0]:
            self.udp_send(addr, {"type": "error",
                                 "text": "UDP rechazado: haz login por TCP"})
            return
        user["seen"] = time.time()

        if kind == "hello":
            user["udp"] = addr
            self.udp_send(addr, {"type": "hello_ok", "users": self.user_list()})
            logging.info("UDP registrado '%s' en %s:%d", name, *addr)
        elif kind == "ping":
            user["udp"] = addr
            self.udp_send(addr, {"type": "pong", "seq": msg.get("seq"),
                                 "ts": msg.get("ts")})
        elif kind == "msg":
            user["udp"] = addr
            text = str(msg.get("text", ""))[:1000]
            to = msg.get("to")
            out = {"type": "msg", "from": name, "text": text, "proto": "UDP",
                   "seq": msg.get("seq"), "ts": time.time()}
            if to:
                out["private"] = True
                out["to"] = to
                target = self.users.get(to)
                if target and target["udp"]:
                    self.udp_send(target["udp"], out)
                    self.udp_send(addr, out)
                else:
                    self.udp_send(addr, {"type": "error",
                                         "text": f"'{to}' no tiene UDP activo"})
            else:
                self.broadcast_udp(out)

    # ----------------------------------------------------------------- bucle
    def serve_forever(self):
        try:
            while True:
                for key, _ in self.sel.select(timeout=1.0):
                    if key.data == "tcp_accept":
                        self.accept()
                    elif key.data == "tcp_client":
                        self.read_tcp(key.fileobj)
                    elif key.data == "udp":
                        self.read_udp()
        except KeyboardInterrupt:
            logging.info("Cerrando servidor. Estadísticas: %s", self.stats)
        finally:
            for s in list(self.sock_to_name):
                s.close()
            self.tcp.close()
            self.udp.close()


conn_buf = {}


def main():
    p = argparse.ArgumentParser(description="Servidor de chat TCP+UDP")
    p.add_argument("--host", default="0.0.0.0")
    p.add_argument("--tcp-port", type=int, default=5000)
    p.add_argument("--udp-port", type=int, default=5001)
    p.add_argument("--max-users", type=int, default=4)
    p.add_argument("--log", default=None, help="archivo de log (opcional)")
    p.add_argument("--verbose", action="store_true",
                   help="registrar también los ping UDP")
    a = p.parse_args()

    handlers = [logging.StreamHandler()]
    if a.log:
        handlers.append(logging.FileHandler(a.log, encoding="utf-8"))
    logging.basicConfig(level=logging.DEBUG if a.verbose else logging.INFO,
                        handlers=handlers,
                        format="%(asctime)s %(levelname)s %(message)s")
    ChatServer(a.host, a.tcp_port, a.udp_port, a.max_users).serve_forever()


if __name__ == "__main__":
    main()
