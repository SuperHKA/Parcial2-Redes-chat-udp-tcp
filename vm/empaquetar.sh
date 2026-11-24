#!/usr/bin/env bash
# Se ejecuta en la MÁQUINA FÍSICA. Compila la interfaz con una glibc antigua
# (contenedor Docker rust:1-bullseye, glibc 2.31) para que funcione en
# Debian 12/13, AlmaLinux 9/10, Ubuntu 22.04+, etc., y crea dist/chat.tar.gz
# con todo lo que necesitan las VMs:
#
#   chat/bin/chat-udp-tcp       interfaz (para las VMs de usuario)
#   chat/server/server.py       servidor (para la VM servidor)
#   chat/vm/*.sh                instaladores
#
# Uso: ./vm/empaquetar.sh
set -euo pipefail
cd "$(dirname "$0")/.."

docker run --rm -u "$(id -u):$(id -g)" \
    -e CARGO_HOME=/work/.cargo-docker -e CARGO_TARGET_DIR=/work/target-compat \
    -v "$PWD/client-rust":/work -w /work \
    rust:1-bullseye cargo build --release

rm -rf dist && mkdir -p dist/chat/bin
cp client-rust/target-compat/release/chat-udp-tcp dist/chat/bin/
cp -r server vm dist/chat/
tar czf dist/chat.tar.gz -C dist chat
echo "Paquete listo: dist/chat.tar.gz"
