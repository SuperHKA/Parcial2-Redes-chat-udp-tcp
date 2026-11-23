#!/usr/bin/env python3
"""Prueba automática: 4 usuarios se conectan y chatean por TCP y por UDP.
Uso: python3 tests/prueba_4_usuarios.py [IP_SERVIDOR]"""
import sys, time, threading
from chatnet import ChatClient

host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
nombres = ["ana", "luis", "maria", "pedro"]
recibido = {n: [] for n in nombres}
lock = threading.Lock()
clientes = {}
for n in nombres:
    def cb(ev, n=n):
        if ev.get("type") == "msg":
            with lock:
                recibido[n].append((ev["proto"], ev["from"], ev["text"], ev.get("private", False)))
    c = ChatClient(host, 5000, 5001, cb)
    c.connect(n)
    clientes[n] = c
time.sleep(0.5)
for n, c in clientes.items():
    c.send(f"hola por TCP de {n}", "TCP")
    c.send(f"hola por UDP de {n}", "UDP")
clientes["ana"].send("secreto para luis", "TCP", to="luis")
clientes["maria"].send("secreto UDP para pedro", "UDP", to="pedro")
time.sleep(1)
ok = True
for n in nombres:
    tcp = sum(1 for p, *_ in recibido[n] if p == "TCP" and not _[2])
    udp = sum(1 for p, *_ in recibido[n] if p == "UDP" and not _[2])
    priv = [r for r in recibido[n] if r[3]]
    print(f"{n:6} grupo TCP={tcp}/4 UDP={udp}/4 privados={[(r[0], r[1], r[2]) for r in priv]}")
    ok &= tcp == 4 and udp == 4
ok &= any(r[2] == "secreto para luis" for r in recibido["luis"])
ok &= not any(r[2] == "secreto para luis" for r in recibido["maria"])
ok &= any(r[2] == "secreto UDP para pedro" for r in recibido["pedro"])
for c in clientes.values():
    c.close()
print("RESULTADO:", "OK" if ok else "FALLO")
sys.exit(0 if ok else 1)
