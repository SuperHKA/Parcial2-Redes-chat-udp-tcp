//! Capa de red: un socket TCP (login, mensajes fiables) y un socket UDP
//! (mensajes sin conexión + latidos/ping) hacia el servidor de la VM.
//! Cada mensaje es un objeto JSON: en TCP uno por línea, en UDP uno por datagrama.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub enum NetEvent {
    Packet(Value),
    Disconnected(String),
}

pub struct Net {
    tcp: TcpStream,
    udp: UdpSocket,
    server_udp: SocketAddr,
    pub name: String,
    seq: u64,
    running: Arc<AtomicBool>,
}

pub struct Session {
    pub net: Net,
    pub rx: Receiver<NetEvent>,
    pub users: Vec<String>,
}

fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("No se pudo resolver {host}: {e}"))?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| format!("{host} no tiene dirección IPv4"))
}

/// Conecta, hace login por TCP, registra el socket UDP y lanza los hilos lectores.
/// `wake` se llama cada vez que llega un paquete (para repintar la interfaz).
pub fn connect(
    host: &str,
    tcp_port: u16,
    udp_port: u16,
    name: &str,
    wake: impl Fn() + Send + Sync + Clone + 'static,
) -> Result<Session, String> {
    let tcp_addr = resolve(host, tcp_port)?;
    let server_udp = resolve(host, udp_port)?;

    let mut tcp = TcpStream::connect_timeout(&tcp_addr, Duration::from_secs(5))
        .map_err(|e| format!("No se pudo conectar a {tcp_addr}: {e}"))?;
    tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let login = json!({"type": "login", "name": name}).to_string() + "\n";
    tcp.write_all(login.as_bytes()).map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(tcp.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("Sin respuesta del servidor: {e}"))?;
    let reply: Value = serde_json::from_str(&line).map_err(|_| "Respuesta inválida".to_string())?;
    if reply["type"] != "welcome" {
        return Err(reply["text"].as_str().unwrap_or("Login rechazado").to_string());
    }
    let name = reply["name"].as_str().unwrap_or(name).to_string();
    let users = str_list(&reply["users"]);
    tcp.set_read_timeout(None).ok();
    reader.get_ref().set_read_timeout(None).ok();

    let udp = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let running = Arc::new(AtomicBool::new(true));
    let (tx, rx) = channel();

    spawn_tcp_reader(reader, tx.clone(), running.clone(), wake.clone());
    spawn_udp_reader(udp.try_clone().map_err(|e| e.to_string())?, tx, running.clone(), wake);
    spawn_heartbeat(
        udp.try_clone().map_err(|e| e.to_string())?,
        server_udp,
        name.clone(),
        running.clone(),
    );

    let net = Net { tcp, udp, server_udp, name, seq: 0, running };
    net.send_udp(&json!({"type": "hello", "name": net.name}));
    Ok(Session { net, rx, users })
}

pub fn str_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn spawn_tcp_reader(
    mut reader: BufReader<TcpStream>,
    tx: Sender<NetEvent>,
    running: Arc<AtomicBool>,
    wake: impl Fn() + Send + 'static,
) {
    thread::spawn(move || loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => {
                if running.swap(false, Ordering::SeqCst) {
                    let _ = tx.send(NetEvent::Disconnected("El servidor cerró la conexión".into()));
                    wake();
                }
                return;
            }
            Ok(_) => {
                if let Ok(mut v) = serde_json::from_str::<Value>(&line) {
                    v["via"] = json!("TCP");
                    let _ = tx.send(NetEvent::Packet(v));
                    wake();
                }
            }
        }
    });
}

fn spawn_udp_reader(
    udp: UdpSocket,
    tx: Sender<NetEvent>,
    running: Arc<AtomicBool>,
    wake: impl Fn() + Send + 'static,
) {
    udp.set_read_timeout(Some(Duration::from_millis(500))).ok();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while running.load(Ordering::SeqCst) {
            if let Ok((n, _)) = udp.recv_from(&mut buf) {
                if let Ok(mut v) = serde_json::from_slice::<Value>(&buf[..n]) {
                    v["via"] = json!("UDP");
                    let _ = tx.send(NetEvent::Packet(v));
                    wake();
                }
            }
        }
    });
}

fn spawn_heartbeat(udp: UdpSocket, server: SocketAddr, name: String, running: Arc<AtomicBool>) {
    thread::spawn(move || {
        let mut seq = 0u64;
        while running.load(Ordering::SeqCst) {
            seq += 1;
            let ping = json!({"type": "ping", "name": name, "seq": seq, "ts": now()});
            let _ = udp.send_to(ping.to_string().as_bytes(), server);
            thread::sleep(Duration::from_secs(3));
        }
    });
}

impl Net {
    fn send_udp(&self, v: &Value) {
        let _ = self.udp.send_to(v.to_string().as_bytes(), self.server_udp);
    }

    fn send_tcp(&mut self, v: &Value) -> Result<(), String> {
        let s = v.to_string() + "\n";
        self.tcp.write_all(s.as_bytes()).map_err(|e| e.to_string())
    }

    /// Envía un mensaje de chat. `to = None` es el grupo; `Some(user)` es privado.
    pub fn send_msg(&mut self, text: &str, udp: bool, to: Option<&str>) -> Result<(), String> {
        if udp {
            self.seq += 1;
            let mut m = json!({"type": "msg", "name": self.name, "text": text,
                               "seq": self.seq, "ts": now()});
            if let Some(t) = to {
                m["to"] = json!(t);
            }
            self.send_udp(&m);
            Ok(())
        } else {
            let mut m = json!({"type": "msg", "text": text});
            if let Some(t) = to {
                m["to"] = json!(t);
            }
            self.send_tcp(&m)
        }
    }

    pub fn close(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            let _ = self.send_tcp(&json!({"type": "quit"}));
            let _ = self.tcp.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl Drop for Net {
    fn drop(&mut self) {
        self.close();
    }
}
