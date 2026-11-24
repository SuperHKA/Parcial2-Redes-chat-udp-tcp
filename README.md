# Chat TCP/UDP entre máquinas virtuales

Chat para hasta 4 usuarios entre máquinas virtuales creadas con virt-manager.
Una VM hace de servidor (el "host principal") y recibe los paquetes TCP y UDP
de los usuarios; cada usuario trabaja en su propia VM con una interfaz en Rust
parecida a WhatsApp. Todo es software libre (licencia MIT): Python 3, Rust,
egui, libvirt/QEMU/KVM y virt-manager.

```
        red NAT "default" de libvirt (192.168.122.0/24, puente virbr0)
   ┌─────────────────────┬──────────────────────┬──────────────────────┐
 Debian13              AlmaLinux              AlmaLinux-2
 servidor              usuario 1              usuario 2
 server.py             interfaz Rust          interfaz Rust
 TCP :5000 UDP :5001
```

La máquina física solo hace de hipervisor. En el log del servidor cada paquete
aparece con la IP de la VM que lo envió.

Documentación breve con capturas: [`docs/Documentacion-Chat-TCP-UDP.docx`](docs/Documentacion-Chat-TCP-UDP.docx).

## Estructura

| Ruta | Contenido |
|---|---|
| `server/server.py` | Servidor TCP+UDP (Python 3, sin dependencias) |
| `client-rust/` | Interfaz gráfica estilo WhatsApp (Rust + egui) |
| `vm/preparar-vm.sh` | Conecta una VM a la red NAT de libvirt, la arranca y da su IP |
| `vm/empaquetar.sh` | Compila la interfaz para las VMs y crea `dist/chat.tar.gz` |
| `vm/instalar-servidor.sh` | Instala el servidor en la VM servidor como servicio systemd |
| `vm/instalar-cliente.sh` | Instala la interfaz en una VM de usuario |
| `docs/` | Documentación en Word y capturas |

## Requisitos

**Máquina física (Linux):**

- KVM/QEMU, libvirt y virt-manager. Tu usuario debe estar en el grupo `libvirt`.
- Docker, para compilar la interfaz de forma compatible con las VMs.
- Rust (`cargo`) solo si quieres compilar la interfaz para la propia máquina física.

**VMs:**

- Servidor: cualquier Linux con systemd y Python 3 (probado con Debian 13).
- Usuarios: Linux con escritorio gráfico (probado con AlmaLinux 10 KDE).

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
- En la interfaz se elige **TCP** o **UDP** antes de enviar. Si está abierto el chat de
  otro usuario, el mensaje es privado. Cada burbuja indica por qué protocolo llegó.

## Puesta en marcha

### 1. Crear las VMs y conectarlas a la red `default`

Hacen falta una VM servidor y al menos dos VMs de usuario. Todas deben estar en la
red virtual `default` (NAT) de libvirt.

Con script (añade una tarjeta en la red `default` si no la tiene, arranca la VM y
muestra su IP):

```bash
./vm/preparar-vm.sh Debian13
```

```bash
./vm/preparar-vm.sh AlmaLinux
```

Desde virt-manager: abre la VM → *Mostrar detalles del hardware virtual* →
*Agregar hardware* → *Red* → Fuente: `Red virtual 'default': NAT`, modelo `virtio`.

Para tener una segunda VM de usuario, clona la primera con ella apagada:

```bash
virt-clone --connect qemu:///system --original AlmaLinux --name AlmaLinux-2 --auto-clone
```

Anota la IP de cada VM (192.168.122.x). La del servidor es la que usarán los usuarios.

### 2. Crear el paquete para las VMs

La interfaz se compila dentro de un contenedor con una glibc antigua. Si se compila
directamente en una distribución reciente (por ejemplo Arch), no arranca en Debian
ni en AlmaLinux porque pide una glibc más nueva.

```bash
./vm/empaquetar.sh
```

La primera vez descarga la imagen `rust:1-bullseye` (unos 1,5 GB).

### 3. Copiar el paquete a cada VM

**Opción A, con SSH** (la VM necesita `openssh-server` instalado y activo):

```bash
scp dist/chat.tar.gz usuario@IP_VM:~/
```

**Opción B, sin SSH:** sirve el paquete desde la máquina física y descárgalo en la VM.
Si la máquina física tiene `ufw` activo, abre el puerto solo en la red de las VMs:

```bash
sudo ufw allow in on virbr0 to 192.168.122.1 port 8000 proto tcp
```

```bash
cd dist && python3 -m http.server 8000 --bind 192.168.122.1
```

En cada VM:

```bash
cd ~ && curl -O http://192.168.122.1:8000/chat.tar.gz
```

Al terminar, para el servidor con Ctrl+C y cierra el puerto:

```bash
sudo ufw delete allow in on virbr0 to 192.168.122.1 port 8000 proto tcp
```

### 4. Instalar el servidor (VM servidor)

```bash
cd ~ && tar xzf chat.tar.gz && sudo ./chat/vm/instalar-servidor.sh
```

Instala el servicio `chat-udp-tcp`, que arranca solo al encender la VM, y abre los
puertos 5000/tcp y 5001/udp si `ufw` o `firewalld` están activos. Para actualizar el
servidor, vuelve a ejecutar el mismo comando con un paquete nuevo.

### 5. Instalar la interfaz (cada VM de usuario)

El argumento es el nombre de la máquina; sirve para distinguir las VMs clonadas:

```bash
cd ~ && tar xzf chat.tar.gz && sudo ./chat/vm/instalar-cliente.sh usuario1
```

En la otra VM usa `usuario2`.

### 6. Chatear

Abre **Chat TCP/UDP** desde el menú de aplicaciones o desde una terminal, con la IP
del servidor y tu nombre:

```bash
chat-udp-tcp 192.168.122.32 ana
```

Sin argumentos aparece la pantalla de login para escribir la IP, los puertos y el nombre.
Cada usuario necesita un nombre distinto.

## Ver los paquetes en el servidor

Log del servicio (login, salidas, mensajes TCP y paquetes UDP recibidos):

```bash
sudo journalctl -fu chat-udp-tcp
```

Los ping UDP no se registran para no llenar el log. Para verlos, añade `--verbose`
a la línea `ExecStart` de `/etc/systemd/system/chat-udp-tcp.service` y ejecuta
`sudo systemctl daemon-reload && sudo systemctl restart chat-udp-tcp`.

Paquetes UDP en crudo:

```bash
sudo tcpdump -i any -nn -X 'udp port 5001'
```

Captura para Wireshark (filtro `tcp.port==5000 || udp.port==5001`):

```bash
sudo tcpdump -i any -w chat.pcap 'tcp port 5000 or udp port 5001'
```

En Wireshark se ve el saludo de tres vías de TCP (SYN, SYN-ACK, ACK) y los ACK de cada
mensaje. En UDP se ven datagramas sueltos, sin conexión ni confirmación.

## Opciones del servidor

```
python3 server/server.py [--host 0.0.0.0] [--tcp-port 5000] [--udp-port 5001]
                         [--max-users 4] [--log archivo] [--verbose]
```

Para probarlo en la máquina física sin VMs:

```bash
python3 server/server.py --host 127.0.0.1
```

```bash
cd client-rust && cargo run --release -- 127.0.0.1 ana
```

## Portapapeles entre la máquina física y las VMs (opcional)

En virt-manager, la pantalla de la VM debe ser **SPICE** (no VNC) y tener un canal
`spicevmc`. En los detalles de la VM: *Pantalla* → Tipo `Servidor Spice`; *Agregar
hardware* → *Canal* → `com.redhat.spice.0` (spicevmc). Dentro de la VM instala
`spice-vdagent`. Después apaga la VM del todo y vuelve a encenderla (reiniciar no basta).

## Problemas frecuentes

| Problema | Causa y solución |
|---|---|
| La interfaz no arranca en la VM y menciona `GLIBC_2.xx` | Se compiló en la máquina física. Usa `./vm/empaquetar.sh`. |
| `wget`/`curl` se queda esperando hasta "connection timed out" | El cortafuegos de la máquina física bloquea la conexión. Ver el paso 3, opción B. |
| "No se pudo conectar" en la interfaz | La VM servidor no está encendida o no está en la red `default`. Comprueba la IP y `systemctl status chat-udp-tcp`. |
| "Sala llena (4)" | Ya hay 4 usuarios. Cambia `--max-users` en el servicio. |
| "Nombre en uso" | Otro usuario ya entró con ese nombre. |
| "UDP sin respuesta" en la cabecera | El puerto 5001/udp está bloqueado en la VM servidor. |
| El botón de enviar o las marcas de entregado se ven como cuadros | A la fuente de la VM le faltan esos símbolos. No afecta al funcionamiento. |

## Licencia

MIT. Ver `LICENSE`.
