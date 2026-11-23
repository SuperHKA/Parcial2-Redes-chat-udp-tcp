#!/usr/bin/env bash
# Se ejecuta en la MÁQUINA FÍSICA (anfitrión de libvirt / virt-manager).
# Conecta la VM a la red NAT "default" de libvirt (192.168.122.0/24) para que
# los clientes del anfitrión y de otras VMs puedan llegar al servidor de chat,
# la arranca y muestra su IP.
#
# Uso: ./vm/preparar-vm.sh [NOMBRE_VM]      (por defecto: Debian13)
set -euo pipefail
VM="${1:-Debian13}"
C="qemu:///system"

if ! virsh -c "$C" domiflist "$VM" | awk 'NR>2{print $3}' | grep -qx default; then
    echo ">> Añadiendo una interfaz virtio en la red 'default' a $VM"
    virsh -c "$C" attach-interface "$VM" network default --model virtio --config
fi

state=$(virsh -c "$C" domstate "$VM")
if [[ "$state" != "running" ]]; then
    echo ">> Arrancando $VM"
    virsh -c "$C" start "$VM"
fi

echo ">> Esperando a que la VM obtenga IP (DHCP de libvirt)..."
for _ in $(seq 1 60); do
    ip=$(virsh -c "$C" domifaddr "$VM" --source lease 2>/dev/null \
         | awk '/192\.168\.122\./{split($4,a,"/"); print a[1]; exit}')
    if [[ -n "${ip:-}" ]]; then
        echo ">> IP de la VM: $ip"
        echo "   Copia el servidor:  scp -r server vm/instalar-servidor.sh usuario@$ip:~/"
        echo "   Abre la interfaz:   ./client-rust/target/release/chat-udp-tcp $ip"
        exit 0
    fi
    sleep 2
done
echo "!! No se obtuvo IP. Abre la VM en virt-manager y revisa la red (ip a)." >&2
exit 1
