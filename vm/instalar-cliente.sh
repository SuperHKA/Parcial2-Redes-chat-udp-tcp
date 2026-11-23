#!/usr/bin/env bash
# Se ejecuta DENTRO de una VM de usuario (AlmaLinux, Debian...).
# Instala la interfaz de chat y crea un acceso directo en el menú.
#
# Uso (en la VM):  sudo ./vm/instalar-cliente.sh [NOMBRE_HOST]
#   NOMBRE_HOST es opcional; sirve para distinguir las VMs clonadas
#   (por ejemplo: usuario1, usuario2).
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$DIR/bin/chat-udp-tcp"
[[ -f "$BIN" ]] || { echo "No encuentro bin/chat-udp-tcp" >&2; exit 1; }

install -m 755 "$BIN" /usr/local/bin/chat-udp-tcp

cat > /usr/share/applications/chat-udp-tcp.desktop <<'EOF'
[Desktop Entry]
Type=Application
Name=Chat TCP/UDP
Comment=Chat entre máquinas virtuales por TCP y UDP
Exec=/usr/local/bin/chat-udp-tcp
Icon=internet-chat
Terminal=false
Categories=Network;Chat;
EOF

if [[ -n "${1:-}" ]]; then
    hostnamectl set-hostname "$1"
    echo "Nombre de la máquina: $1"
fi

echo
echo "Cliente instalado. Ábrelo desde el menú (Chat TCP/UDP) o con:"
echo "  chat-udp-tcp 192.168.122.32 tu_nombre"
echo "IPs de esta VM:"; ip -4 -o addr show scope global | awk '{print "  " $4}'
