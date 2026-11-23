#!/usr/bin/env bash
# Se ejecuta DENTRO de la VM (Debian/Ubuntu/Alma). Instala el servidor de chat
# como servicio systemd, abre los puertos 5000/tcp y 5001/udp y lo arranca.
#
# Uso (en la VM, como root):  sudo ./instalar-servidor.sh
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$DIR/server/server.py"
[[ -f "$SRC" ]] || SRC="$DIR/../server/server.py"
[[ -f "$SRC" ]] || { echo "No encuentro server/server.py" >&2; exit 1; }

command -v python3 >/dev/null || {
    if command -v apt-get >/dev/null; then apt-get update && apt-get install -y python3
    else dnf install -y python3; fi
}

install -d /opt/chat-udp-tcp
install -m 755 "$SRC" /opt/chat-udp-tcp/server.py

cat > /etc/systemd/system/chat-udp-tcp.service <<'EOF'
[Unit]
Description=Servidor de chat TCP+UDP
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/bin/python3 /opt/chat-udp-tcp/server.py --tcp-port 5000 --udp-port 5001 --max-users 4 --log /var/log/chat-udp-tcp/chat.log
Restart=on-failure
DynamicUser=yes
LogsDirectory=chat-udp-tcp

[Install]
WantedBy=multi-user.target
EOF

# Cortafuegos (solo si hay uno activo)
if command -v ufw >/dev/null && ufw status | grep -q active; then
    ufw allow 5000/tcp && ufw allow 5001/udp
elif command -v firewall-cmd >/dev/null && firewall-cmd --state >/dev/null 2>&1; then
    firewall-cmd --permanent --add-port=5000/tcp --add-port=5001/udp && firewall-cmd --reload
fi

systemctl daemon-reload
systemctl enable chat-udp-tcp
systemctl restart chat-udp-tcp
systemctl --no-pager status chat-udp-tcp | head -5
echo
echo "Servidor activo. IPs de esta VM:"; ip -4 -o addr show scope global | awk '{print "  " $4}'
echo "Ver paquetes en directo:  journalctl -fu chat-udp-tcp"
echo "Capturar para Wireshark:  sudo tcpdump -i any -w chat.pcap 'tcp port 5000 or udp port 5001'"
