# Chat TCP/UDP con VM (virt-manager)

Chat para hasta 4 usuarios. El servidor corre en una máquina virtual (el "host
principal") y recibe los paquetes TCP y UDP de los usuarios. La interfaz está
hecha en Rust y se parece a WhatsApp. Todo es software libre (licencia MIT):
Python 3, Rust, egui, libvirt/QEMU/KVM y virt-manager.

```
 ┌───────────── Máquina física (virt-manager / libvirt) ─────────────┐
 │                                                                   │
 │  Cliente Rust (ana)   Cliente Rust (luis)  ...                    │
 │        │  TCP 5000 + UDP 5001                                     │
 │        ▼                                                          │
 │   virbr0  192.168.122.1  (red NAT "default" de libvirt)           │
 │        │                                                          │
 │   ┌────┴──────────── VM Debian13 ──────────────┐                  │
 │   │ server.py  TCP :5000  UDP :5001            │                  │
 │   │ log de cada paquete UDP recibido           │                  │
 │   └────────────────────────────────────────────┘                  │
 └───────────────────────────────────────────────────────────────────┘
```

## Estructura

| Carpeta | Contenido |
|---|---|
| `server/server.py` | Servidor TCP+UDP (Python 3, sin dependencias) |
| `client-rust/` | Interfaz gráfica estilo WhatsApp (Rust + egui) |
| `vm/preparar-vm.sh` | Conecta la VM a la red NAT de libvirt, la arranca y da su IP |
| `vm/instalar-servidor.sh` | Instala el servidor en la VM como servicio systemd |


## Protocolo

Cada mensaje es un objeto JSON. En TCP va uno por línea; en UDP, uno por datagrama.

| Protocolo | Mensaje del cliente | Respuesta del servidor |
|---|---|---|
| TCP | `{"type":"login","name":"ana"}` | `welcome` + aviso `system` a todos |
| TCP | `{"type":"msg","text":"hola"}` (`"to":"luis"` si es privado) | `msg` con `"proto":"TCP"` |
| UDP | `{"type":"hello","name":"ana"}` | `hello_ok` (registra el puerto UDP) |
| UDP | `{"type":"msg","name":"ana","text":"hola","seq":1}` | `msg` con `"proto":"UDP"` |
| UDP | `{"type":"ping","name":"ana","ts":...}` cada 3 s | `pong` (la interfaz muestra la latencia) |

- **TCP** se usa para el login, la lista de usuarios y los mensajes fiables.
- **UDP** se usa para mensajes rápidos sin garantía de entrega, y para el ping.
- El servidor solo acepta UDP de usuarios que ya hicieron login por TCP desde la misma IP.
- En la interfaz se elige **TCP** o **UDP** antes de enviar. Cada burbuja indica por qué protocolo llegó.

## Puesta en marcha

### 1. Compilar la interfaz (máquina física)

```bash
cd client-rust && cargo build --release
```

### 2. Preparar la VM con virt-manager

Con script (usa la VM `Debian13` y le añade una tarjeta en la red `default`):

```bash
./vm/preparar-vm.sh Debian13
```

Desde la interfaz gráfica de virt-manager: abre la VM → *Mostrar detalles del
hardware virtual* → *Agregar hardware* → *Red* → Fuente: `Red virtual 'default': NAT`,
modelo `virtio`. Arranca la VM y anota su IP con `ip a` (192.168.122.x).

### 3. Instalar el servidor dentro de la VM

```bash
scp -r server vm/instalar-servidor.sh usuario@IP_VM:~/
```

En la VM:

```bash
sudo ./instalar-servidor.sh
```

```bash
journalctl -fu chat-udp-tcp
```

### 4. Chatear

En la máquina física, abre una ventana por usuario (o usa otras VMs u otros PCs de la red):

```bash
./client-rust/target/release/chat-udp-tcp IP_VM ana
```

```bash
./client-rust/target/release/chat-udp-tcp IP_VM luis
```

Sin argumentos aparece la pantalla de login para escribir la IP y el nombre.

## Ver los paquetes (evidencia para el informe)

En la VM:

```bash
sudo tcpdump -i any -nn -X 'udp port 5001'
```

O guarda una captura y ábrela en Wireshark con el filtro `tcp.port==5000 || udp.port==5001`:

```bash
sudo tcpdump -i any -w chat.pcap 'tcp port 5000 or udp port 5001'
```

En Wireshark se ve el saludo de tres vías de TCP (SYN, SYN-ACK, ACK) y los ACK de cada
mensaje. En UDP se ven datagramas sueltos, sin conexión ni confirmación.

## Licencia

MIT. Ver `LICENSE`.
