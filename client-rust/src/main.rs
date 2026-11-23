//! Cliente de chat estilo WhatsApp sobre TCP y UDP (egui/eframe).
//! Licencia: MIT.

mod net;

use eframe::egui::{
    self, Align, Color32, FontId, Layout, Margin, RichText, Rounding, Stroke, Vec2,
};
use net::{NetEvent, Session};
use serde_json::Value;
use std::collections::HashMap;

// Paleta inspirada en WhatsApp.
const VERDE: Color32 = Color32::from_rgb(0x07, 0x5E, 0x54);
const VERDE_CLARO: Color32 = Color32::from_rgb(0x25, 0xD3, 0x66);
const FONDO_CHAT: Color32 = Color32::from_rgb(0xEC, 0xE5, 0xDD);
const BURBUJA_MIA: Color32 = Color32::from_rgb(0xD9, 0xFD, 0xD3);
const BURBUJA_OTRO: Color32 = Color32::WHITE;
const GRIS: Color32 = Color32::from_rgb(0x66, 0x77, 0x81);
const GRUPO: &str = ""; // clave del chat de grupo

struct Mensaje {
    de: String,
    texto: String,
    proto: String,
    hora: String,
    mio: bool,
    sistema: bool,
}

struct Login {
    host: String,
    tcp: String,
    udp: String,
    nombre: String,
    error: Option<String>,
}

struct App {
    login: Login,
    sesion: Option<Session>,
    usuarios: Vec<String>,
    chats: HashMap<String, Vec<Mensaje>>,
    no_leidos: HashMap<String, usize>,
    actual: String,
    entrada: String,
    usar_udp: bool,
    udp_ok: bool,
    ping_ms: Option<f64>,
    enviados: (u64, u64),
    recibidos: (u64, u64),
    auto_intentado: bool,
}

fn hora(ts: f64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts as i64, 0)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Color estable por nombre, como los nombres de colores en grupos de WhatsApp.
fn color_nombre(nombre: &str) -> Color32 {
    const C: [Color32; 6] = [
        Color32::from_rgb(0xD3, 0x2F, 0x2F),
        Color32::from_rgb(0x19, 0x76, 0xD2),
        Color32::from_rgb(0x7B, 0x1F, 0xA2),
        Color32::from_rgb(0xF5, 0x7C, 0x00),
        Color32::from_rgb(0x00, 0x89, 0x7B),
        Color32::from_rgb(0xC2, 0x18, 0x5B),
    ];
    let h = nombre.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    C[(h as usize) % C.len()]
}

fn avatar(ui: &mut egui::Ui, nombre: &str, tam: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(tam), egui::Sense::hover());
    let color = if nombre.is_empty() { VERDE_CLARO } else { color_nombre(nombre) };
    ui.painter().circle_filled(rect.center(), tam / 2.0, color);
    let letra = if nombre.is_empty() {
        "👥".to_string()
    } else {
        nombre.chars().next().unwrap().to_uppercase().to_string()
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letra,
        FontId::proportional(tam * 0.5),
        Color32::WHITE,
    );
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut args = std::env::args().skip(1);
        let host = args.next().unwrap_or_else(|| "192.168.122.10".into());
        let nombre = args.next().unwrap_or_default();
        Self {
            login: Login {
                host,
                tcp: "5000".into(),
                udp: "5001".into(),
                nombre,
                error: None,
            },
            sesion: None,
            usuarios: vec![],
            chats: HashMap::new(),
            no_leidos: HashMap::new(),
            actual: GRUPO.into(),
            entrada: String::new(),
            usar_udp: false,
            udp_ok: false,
            ping_ms: None,
            enviados: (0, 0),
            recibidos: (0, 0),
            auto_intentado: false,
        }
    }

    fn yo(&self) -> &str {
        self.sesion.as_ref().map(|s| s.net.name.as_str()).unwrap_or("")
    }

    fn agregar(&mut self, chat: String, m: Mensaje) {
        if chat != self.actual && !m.sistema {
            *self.no_leidos.entry(chat.clone()).or_default() += 1;
        }
        self.chats.entry(chat).or_default().push(m);
    }

    fn sistema(&mut self, chat: String, texto: String) {
        self.agregar(chat, Mensaje {
            de: String::new(), texto, proto: String::new(),
            hora: hora(net::now()), mio: false, sistema: true,
        });
    }

    // ------------------------------------------------------------- red
    fn procesar_red(&mut self) {
        let mut eventos = vec![];
        if let Some(s) = &self.sesion {
            while let Ok(e) = s.rx.try_recv() {
                eventos.push(e);
            }
        }
        for e in eventos {
            match e {
                NetEvent::Disconnected(motivo) => {
                    self.sesion = None;
                    self.login.error = Some(motivo);
                    return;
                }
                NetEvent::Packet(v) => self.paquete(v),
            }
        }
    }

    fn paquete(&mut self, v: Value) {
        let via = v["via"].as_str().unwrap_or("TCP").to_string();
        match v["type"].as_str().unwrap_or("") {
            "msg" => {
                if via == "UDP" { self.recibidos.1 += 1 } else { self.recibidos.0 += 1 }
                let de = v["from"].as_str().unwrap_or("?").to_string();
                let mio = de == self.yo();
                let chat = if v["private"].as_bool().unwrap_or(false) {
                    if mio { v["to"].as_str().unwrap_or("").to_string() } else { de.clone() }
                } else {
                    GRUPO.to_string()
                };
                let m = Mensaje {
                    de, mio, sistema: false,
                    texto: v["text"].as_str().unwrap_or("").to_string(),
                    proto: v["proto"].as_str().unwrap_or(&via).to_string(),
                    hora: hora(v["ts"].as_f64().unwrap_or_else(net::now)),
                };
                self.agregar(chat, m);
            }
            "system" => {
                if v.get("users").is_some() {
                    self.usuarios = net::str_list(&v["users"]);
                }
                if let Some(t) = v["text"].as_str() {
                    if t != "Usuarios conectados" {
                        self.sistema(GRUPO.into(), t.to_string());
                    }
                }
            }
            "error" => {
                let t = format!("⚠ {}", v["text"].as_str().unwrap_or("error"));
                let chat = self.actual.clone();
                self.sistema(chat, t);
            }
            "hello_ok" => self.udp_ok = true,
            "pong" => {
                self.udp_ok = true;
                if let Some(ts) = v["ts"].as_f64() {
                    self.ping_ms = Some((net::now() - ts) * 1000.0);
                }
            }
            _ => {}
        }
    }

    fn enviar(&mut self) {
        let texto = self.entrada.trim().to_string();
        if texto.is_empty() {
            return;
        }
        let to = if self.actual == GRUPO { None } else { Some(self.actual.clone()) };
        let udp = self.usar_udp;
        if let Some(s) = &mut self.sesion {
            match s.net.send_msg(&texto, udp, to.as_deref()) {
                Ok(()) => {
                    if udp { self.enviados.1 += 1 } else { self.enviados.0 += 1 }
                    self.entrada.clear();
                }
                Err(e) => self.login.error = Some(e),
            }
        }
    }

    // ---------------------------------------------------------- pantallas
    fn pantalla_login(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(FONDO_CHAT))
            .show(ctx, |ui| {
                ui.add_space(ui.available_height() * 0.12);
                ui.vertical_centered(|ui| {
                    egui::Frame::none()
                        .fill(Color32::WHITE)
                        .rounding(Rounding::same(12.0))
                        .inner_margin(Margin::same(28.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 2.0), blur: 12.0, spread: 0.0,
                            color: Color32::from_black_alpha(30),
                        })
                        .show(ui, |ui| {
                            ui.set_width(320.0);
                            ui.vertical_centered(|ui| {
                                avatar(ui, "", 64.0);
                                ui.add_space(8.0);
                                ui.label(RichText::new("Chat TCP / UDP").size(22.0).strong().color(VERDE));
                                ui.label(RichText::new("Conéctate al servidor de la VM").color(GRIS));
                            });
                            ui.add_space(16.0);
                            let campo = |ui: &mut egui::Ui, etiqueta: &str, valor: &mut String| {
                                ui.label(RichText::new(etiqueta).small().color(GRIS));
                                ui.add(egui::TextEdit::singleline(valor).desired_width(f32::INFINITY).margin(Margin::symmetric(8.0, 6.0)));
                                ui.add_space(6.0);
                            };
                            campo(ui, "Tu nombre", &mut self.login.nombre);
                            campo(ui, "IP del servidor (VM)", &mut self.login.host);
                            ui.columns(2, |c| {
                                campo(&mut c[0], "Puerto TCP", &mut self.login.tcp);
                                campo(&mut c[1], "Puerto UDP", &mut self.login.udp);
                            });
                            if let Some(e) = &self.login.error {
                                ui.colored_label(Color32::from_rgb(0xC6, 0x28, 0x28), e);
                                ui.add_space(6.0);
                            }
                            let boton = egui::Button::new(RichText::new("Entrar").color(Color32::WHITE).size(16.0))
                                .fill(VERDE_CLARO)
                                .rounding(Rounding::same(20.0))
                                .min_size(Vec2::new(ui.available_width(), 38.0));
                            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.add(boton).clicked() || enter {
                                self.conectar(ctx);
                            }
                        });
                });
            });
    }

    fn conectar(&mut self, ctx: &egui::Context) {
        let (Ok(tcp), Ok(udp)) = (self.login.tcp.trim().parse(), self.login.udp.trim().parse()) else {
            self.login.error = Some("Puertos no válidos".into());
            return;
        };
        let ctx2 = ctx.clone();
        match net::connect(self.login.host.trim(), tcp, udp, self.login.nombre.trim(), move || ctx2.request_repaint()) {
            Ok(s) => {
                self.usuarios = s.users.clone();
                self.sesion = Some(s);
                self.login.error = None;
                self.chats.clear();
                self.no_leidos.clear();
                self.actual = GRUPO.into();
                self.udp_ok = false;
                self.ping_ms = None;
                self.enviados = (0, 0);
                self.recibidos = (0, 0);
            }
            Err(e) => self.login.error = Some(e),
        }
    }

    fn lista_chats(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("chats")
            .exact_width(250.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(Color32::WHITE))
            .show(ctx, |ui| {
                // cabecera
                egui::Frame::none().fill(VERDE).inner_margin(Margin::symmetric(14.0, 12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        avatar(ui, &self.yo().to_string(), 36.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(self.yo()).color(Color32::WHITE).strong().size(16.0));
                            ui.label(RichText::new("en línea").color(Color32::from_white_alpha(200)).small());
                        });
                    });
                });
                let mut chats = vec![GRUPO.to_string()];
                let yo = self.yo().to_string();
                chats.extend(self.usuarios.iter().filter(|u| **u != yo).cloned());
                for c in chats {
                    self.fila_chat(ui, &c);
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.add_space(8.0);
                    let r = |(t, u): (u64, u64)| format!("TCP {t} · UDP {u}");
                    ui.horizontal(|ui| { ui.add_space(12.0); ui.label(RichText::new(format!("↓ recibidos  {}", r(self.recibidos))).small().color(GRIS)); });
                    ui.horizontal(|ui| { ui.add_space(12.0); ui.label(RichText::new(format!("↑ enviados   {}", r(self.enviados))).small().color(GRIS)); });
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        if ui.button("Salir").clicked() {
                            if let Some(mut s) = self.sesion.take() { s.net.close(); }
                        }
                    });
                });
            });
    }

    fn fila_chat(&mut self, ui: &mut egui::Ui, chat: &str) {
        let activo = self.actual == chat;
        let titulo = if chat == GRUPO { format!("Grupo ({})", self.usuarios.len()) } else { chat.to_string() };
        let ultimo = self.chats.get(chat).and_then(|v| v.last())
            .map(|m| if m.sistema || m.de.is_empty() { m.texto.clone() } else { format!("{}: {}", if m.mio { "Tú" } else { &m.de }, m.texto) })
            .unwrap_or_else(|| if chat == GRUPO { "Todos los usuarios".into() } else { "Chat privado".into() });
        let no_leidos = self.no_leidos.get(chat).copied().unwrap_or(0);

        let fondo = if activo { Color32::from_rgb(0xF0, 0xF2, 0xF5) } else { Color32::WHITE };
        let resp = egui::Frame::none().fill(fondo).inner_margin(Margin::symmetric(12.0, 10.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                avatar(ui, chat, 42.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&titulo).strong().size(15.0));
                        if no_leidos > 0 {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                egui::Frame::none().fill(VERDE_CLARO).rounding(Rounding::same(10.0))
                                    .inner_margin(Margin::symmetric(7.0, 1.0))
                                    .show(ui, |ui| ui.label(RichText::new(no_leidos.to_string()).color(Color32::WHITE).small().strong()));
                            });
                        }
                    });
                    let corto: String = ultimo.chars().take(28).collect();
                    let corto = if ultimo.chars().count() > 28 { corto + "…" } else { corto };
                    ui.label(RichText::new(corto).color(GRIS).small());
                });
            });
        }).response;
        let resp = ui.interact(resp.rect, ui.id().with(("chat", chat)), egui::Sense::click());
        if resp.clicked() {
            self.actual = chat.to_string();
            self.no_leidos.remove(chat);
        }
        if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        ui.painter().hline(resp.rect.x_range().shrink(12.0), resp.rect.bottom(), Stroke::new(0.5, Color32::from_gray(225)));
    }

    fn cabecera_chat(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("cabecera")
            .frame(egui::Frame::none().fill(Color32::from_rgb(0xF0, 0xF2, 0xF5)).inner_margin(Margin::symmetric(14.0, 10.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    avatar(ui, &self.actual.clone(), 38.0);
                    ui.vertical(|ui| {
                        let titulo = if self.actual == GRUPO { "Grupo".to_string() } else { self.actual.clone() };
                        ui.label(RichText::new(titulo).strong().size(16.0));
                        let sub = if self.actual == GRUPO { self.usuarios.join(", ") } else { "privado".into() };
                        ui.label(RichText::new(sub).small().color(GRIS));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (txt, col) = match (self.udp_ok, self.ping_ms) {
                            (true, Some(p)) => (format!("● UDP ping {p:.1} ms"), VERDE_CLARO),
                            (true, None) => ("● UDP activo".into(), VERDE_CLARO),
                            _ => ("● UDP sin respuesta".into(), Color32::from_rgb(0xE5, 0x39, 0x35)),
                        };
                        ui.label(RichText::new(txt).small().color(col));
                        ui.label(RichText::new("● TCP conectado  ").small().color(VERDE_CLARO));
                    });
                });
            });
    }

    fn barra_entrada(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("entrada")
            .frame(egui::Frame::none().fill(Color32::from_rgb(0xF0, 0xF2, 0xF5)).inner_margin(Margin::symmetric(12.0, 10.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // selector de protocolo
                    for (udp, nombre) in [(false, "TCP"), (true, "UDP")] {
                        let sel = self.usar_udp == udp;
                        let b = egui::Button::new(RichText::new(nombre).small().strong()
                            .color(if sel { Color32::WHITE } else { GRIS }))
                            .fill(if sel { VERDE } else { Color32::WHITE })
                            .rounding(Rounding::same(12.0));
                        if ui.add(b).on_hover_text(if udp { "Datagrama sin conexión: rápido, sin garantía de entrega" } else { "Conexión fiable: entrega y orden garantizados" }).clicked() {
                            self.usar_udp = udp;
                        }
                    }
                    let ancho = ui.available_width() - 52.0;
                    let r = egui::Frame::none().fill(Color32::WHITE).rounding(Rounding::same(20.0))
                        .inner_margin(Margin::symmetric(12.0, 6.0)).show(ui, |ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.entrada)
                                .hint_text("Escribe un mensaje")
                                .frame(false)
                                .desired_width(ancho - 24.0)
                                .font(FontId::proportional(15.0)))
                        }).inner;
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let boton = egui::Button::new(RichText::new("➤").size(18.0).color(Color32::WHITE))
                        .fill(VERDE_CLARO).rounding(Rounding::same(20.0)).min_size(Vec2::splat(40.0));
                    if ui.add(boton).clicked() || enter {
                        self.enviar();
                        r.request_focus();
                    }
                });
            });
    }

    fn mensajes(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(FONDO_CHAT).inner_margin(Margin::symmetric(16.0, 8.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
                    let max_w = (ui.available_width() * 0.65).max(160.0);
                    let es_grupo = self.actual == GRUPO;
                    let vacio = vec![];
                    let lista = self.chats.get(&self.actual).unwrap_or(&vacio);
                    if lista.is_empty() {
                        ui.add_space(20.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("No hay mensajes todavía. ¡Saluda! 👋").color(GRIS)));
                    }
                    for m in lista {
                        ui.add_space(4.0);
                        if m.sistema {
                            ui.vertical_centered(|ui| {
                                egui::Frame::none().fill(Color32::from_rgb(0xFF, 0xF5, 0xC4)).rounding(Rounding::same(6.0))
                                    .inner_margin(Margin::symmetric(10.0, 4.0))
                                    .show(ui, |ui| ui.label(RichText::new(&m.texto).small().color(Color32::from_rgb(0x54, 0x65, 0x6F))));
                            });
                            continue;
                        }
                        let layout = if m.mio { Layout::right_to_left(Align::TOP) } else { Layout::left_to_right(Align::TOP) };
                        ui.with_layout(layout, |ui| {
                            egui::Frame::none()
                                .fill(if m.mio { BURBUJA_MIA } else { BURBUJA_OTRO })
                                .rounding(Rounding { nw: if m.mio { 8.0 } else { 0.0 }, ne: if m.mio { 0.0 } else { 8.0 }, sw: 8.0, se: 8.0 })
                                .inner_margin(Margin::symmetric(10.0, 6.0))
                                .shadow(egui::epaint::Shadow { offset: Vec2::new(0.0, 1.0), blur: 1.0, spread: 0.0, color: Color32::from_black_alpha(25) })
                                .show(ui, |ui| {
                                    ui.set_max_width(max_w);
                                    ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                                        if es_grupo && !m.mio {
                                            ui.label(RichText::new(&m.de).strong().small().color(color_nombre(&m.de)));
                                        }
                                        ui.add(egui::Label::new(RichText::new(&m.texto).size(15.0).color(Color32::from_rgb(0x11, 0x1B, 0x21))).wrap());
                                        ui.horizontal(|ui| {
                                            let pcol = if m.proto == "UDP" { Color32::from_rgb(0xF5, 0x7C, 0x00) } else { Color32::from_rgb(0x19, 0x76, 0xD2) };
                                            ui.label(RichText::new(&m.proto).size(10.0).strong().color(pcol));
                                            let tick = if m.mio { " ✓✓" } else { "" };
                                            ui.label(RichText::new(format!("{}{}", m.hora, tick)).size(10.0).color(GRIS));
                                        });
                                    });
                                });
                        });
                    }
                    ui.add_space(6.0);
                });
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // `chat-udp-tcp <ip> <nombre>` entra directamente sin pulsar "Entrar".
        if !self.auto_intentado {
            self.auto_intentado = true;
            if !self.login.nombre.is_empty() {
                self.conectar(ctx);
            }
        }
        self.procesar_red();
        if self.sesion.is_none() {
            self.pantalla_login(ctx);
            return;
        }
        self.lista_chats(ctx);
        self.cabecera_chat(ctx);
        self.barra_entrada(ctx);
        self.mensajes(ctx);
        // repintado periódico para refrescar el ping
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
}

fn main() -> eframe::Result<()> {
    let opciones = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Chat TCP/UDP")
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native("Chat TCP/UDP", opciones, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
