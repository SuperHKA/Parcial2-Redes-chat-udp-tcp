#!/usr/bin/env python3
"""Lanza 3 usuarios simulados (ana, luis, maria) que conversan durante un rato,
para probar la interfaz con 4 usuarios en total.
Uso: python3 tests/bots_demo.py [IP_SERVIDOR] [TU_NOMBRE] [SEGUNDOS]"""
import sys, time
from chatnet import ChatClient

host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
yo = sys.argv[2] if len(sys.argv) > 2 else "andre"
dura = float(sys.argv[3]) if len(sys.argv) > 3 else 60

bots = {n: ChatClient(host, 5000, 5001, lambda ev: None) for n in ("ana", "luis", "maria")}
for n, c in bots.items():
    c.connect(n)
    time.sleep(0.3)

guion = [
    ("ana", "TCP", None, "¡Hola a todos! 👋"),
    ("luis", "UDP", None, "Hola, este mensaje va por UDP"),
    ("maria", "TCP", None, "Yo uso TCP, llega seguro y en orden"),
    ("luis", "UDP", None, "UDP es más rápido pero puede perder paquetes"),
    ("ana", "TCP", yo, f"Hola {yo}, esto es un mensaje privado por TCP"),
    ("maria", "UDP", yo, "Y este es privado por UDP 🙂"),
]
for quien, proto, to, texto in guion:
    time.sleep(1.2)
    bots[quien].send(texto, proto, to=to)

time.sleep(dura)
for c in bots.values():
    c.close()
