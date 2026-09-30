//! katana-rs: modern Rust editor for Boss Katana MK2 (FW2).
//!
//! Protocol ported from KATANA-MK2-FxFloorBoard (GPL-2.0).
//! UI: egui/eframe. MIDI: midir (ALSA).

use katana_rs_proto::agent;
use katana_rs_proto::live;
use katana_rs_proto::amp;
use katana_rs_proto::midi;
use katana_rs_proto::patch;
use katana_rs_proto::protocol;
use katana_rs_proto::schema;

use std::sync::mpsc::{channel, Receiver};
use anyhow::Result as AResult;
use agent::{apply_sparse_changes, apply_tsl_json, parse_lenient_json, system_prompt, Agent, AgentConfig, Provider};
use amp::AmpApi;
use katana_rs_proto::knob::{knob, KnobSpec};
use midi::{BusEvent, MidiBus, PortInfo};
use patch::Patch;
use schema::{Schema, ValueSpec};

// ---------- theme ----------
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x5C, 0xA6, 0xE0);
const BG: egui::Color32 = egui::Color32::from_rgb(0x14, 0x14, 0x18);
const CARD: egui::Color32 = egui::Color32::from_rgb(0x1E, 0x1E, 0x26);
const TEXT: egui::Color32 = egui::Color32::from_rgb(0xD8, 0xD8, 0xE2);
const DIM: egui::Color32 = egui::Color32::from_rgb(0x8A, 0x8A, 0x96);

fn main() -> eframe::Result<()> {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([1024.0, 640.0])
            .with_title("Katana MK2 Editor"),
        ..Default::default()
    };
    eframe::run_native(
        "katana-mk2-editor",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// App-level state.
struct App {
    schema: Schema,
    bus: MidiBus,
    connect: ConnectState,
    editor: EditorState,
    amp: AmpApi,
    agent: AgentState,
    /// Cached port listing (refreshed by the Refresh button).
    ports: (Vec<PortInfo>, Vec<PortInfo>),
    sel_in: Option<usize>,
    sel_out: Option<usize>,
    sync_busy: bool,
}

enum ConnectState {
    Idle,
    Connected,
    Failed(String),
}

struct EditorState {
    patch: Patch,
    page: Page,
    status: String,
    bank: usize,
    slot: usize,
    /// incoming sysex assembly buffer for patch replies.
    rx_buf: Vec<u8>,
    /// number of frame bytes assembled so far (expected 2058).
    rx_frames: Vec<u8>,
    /// Live frames received while a patch reply was in flight; re-applied
    /// after the merge so amp knob changes aren't clobbered.
    pending_live: Vec<protocol::Frame>,
}

/// LLM agent UI state: config + loaded conversation, and the receiver for
/// the background request thread.
struct AgentState {
    config: AgentConfig,
    messages: Vec<(String, String)>,
    input: String,
    pending: bool,
    rx: Option<Receiver<AResult<String>>>,
    last_changes: Option<serde_json::Value>,
    status: String,
}

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Amp,
    EQ,
    FX1,
    FX2,
    DelayReverb,
    Output,
    Assign,
    Chat,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let schema = Schema::load(include_str!("../../assets/midi.xml"))
            .expect("midi.xml must be valid");
        let tpl = include_bytes!("../../assets/default.syx");
        let patch = Patch::from_template(tpl, "New Patch".into());
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = CARD;
        style.visuals.widgets.noninteractive.bg_fill = CARD;
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x2A, 0x2A, 0x36);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x33, 0x3A, 0x4A);
        style.visuals.selection.bg_fill = ACCENT;
        cc.egui_ctx.set_style(style);
        let bus = MidiBus::spawn();
        let amp = AmpApi::new(&bus);
        let config = AgentConfig::load();
        let status = format!(
            "agent: {} / {}",
            match config.provider {
                Provider::OpenAI => "OpenAI",
                Provider::Compatible => "OpenAI-compatible",
            },
            config.model
        );
        let (ins, outs) = MidiBus::list_ports().unwrap_or_default();
        let sel_in = ins.iter().position(|p| p.is_katanaport);
        let sel_out = outs.iter().position(|p| p.is_katanaport);
        if let (Some(i), Some(o)) = (sel_in, sel_out) {
            let iname = ins[i].name.clone();
            let oname = outs[o].name.clone();
            bus.set_device(iname, oname);
        }
        App {
            schema,
            bus,
            connect: ConnectState::Idle,
            editor: EditorState {
                patch,
                page: Page::Amp,
                status: String::new(),
                bank: 1,
                slot: 1,
                rx_buf: Vec::new(),
                rx_frames: Vec::new(),
                pending_live: Vec::new(),
            },
            amp,
            agent: AgentState {
                config,
                messages: Vec::new(),
                input: String::new(),
                pending: false,
                rx: None,
                last_changes: None,
                status,
            },
            ports: (ins, outs),
            sel_in,
            sel_out,
            sync_busy: false,
        }
    }

    fn pump_midi(&mut self) {
        while let Some(ev) = self.bus.try_recv() {
            match ev {
                BusEvent::Connected => {
                    self.connect = ConnectState::Connected;
                    self.editor.status = "Connected to Katana".into();
                    // Request the currently-selected patch so the editor shows
                    // the amp's live config.
                    let _ = self.amp.request_patch(self.editor.bank, 1);
                    self.amp.enable_verbose();
                }
                BusEvent::Disconnected(reason) => {
                    self.connect = ConnectState::Failed(reason);
                }
                BusEvent::Received(data) => {
                    self.editor.rx_buf.extend_from_slice(&data);
                    self.assemble_rx();
                }
            }
        }
    }

    /// Extract complete F0..F7 frames from the rx buffer; once a full patch
    /// reply (2058 bytes) has been assembled, merge into the current patch.
    fn assemble_rx(&mut self) {
        let mut i = 0;
        while i < self.editor.rx_buf.len() {
            if self.editor.rx_buf[i] == 0xF0 {
                if let Some(pos) = self.editor.rx_buf[i..].iter().position(|&b| b == 0xF7) {
                    let end = i + pos + 1;
                    let frame_bytes = self.editor.rx_buf[i..end].to_vec();
                    self.editor.rx_buf.drain(..end);
                    if let Some((f, _)) = protocol::Frame::parse(&frame_bytes) {
                        if live::is_live_frame(&f) && f.data.len() <= 32 {
                            if !self.editor.rx_frames.is_empty()
                                && self.editor.rx_frames.len() < protocol::PATCH_REPLY_SIZE
                            {
                                self.editor.pending_live.push(f);
                            } else {
                                live::apply_live(&mut self.editor.patch, &f);
                                self.editor.status = "amp change applied".into();
                            }
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
        if self.editor.rx_frames.len() >= protocol::PATCH_REPLY_SIZE {
            let tpl = include_bytes!("../../assets/default.syx");
            let reply = std::mem::take(&mut self.editor.rx_frames);
            self.editor.rx_buf.clear();
            let merged = Patch::from_reply(tpl, &reply);
            self.editor.patch.frames = merged.frames;
            for f in std::mem::take(&mut self.editor.pending_live) {
                live::apply_live(&mut self.editor.patch, &f);
            }
            self.editor.patch.dirty = false;
            let name = String::from_utf8_lossy(&self.editor.patch.name_bytes())
                .trim_end()
                .to_string();
            self.editor.status = format!("patch loaded from amp: '{name}'");
        }
    }

    /// Apply the latest agent `changes` onto the current patch, push it to
    /// the amp temp buffer, and mark it dirty.
    fn apply_agent_changes(&mut self) {
        let Some(changes) = self.agent.last_changes.clone() else { return };
        let result = if changes.get("changes").map(|c| c.is_array()).unwrap_or(false) {
            apply_sparse_changes(
                &mut self.editor.patch,
                changes.get("changes").expect("checked above"),
            )
        } else {
            apply_tsl_json(&mut self.editor.patch, &changes)
        };
        match result {
            Ok(n) => {
                self.editor.patch.dirty = true;
                self.amp.push_patch(&self.editor.patch);
                self.agent.status = format!("agent: applied {n} parameter sets to amp");
                self.editor.status = "agent patch applied to amp".into();
            }
            Err(e) => {
                self.agent.status = format!("agent: apply failed: {e}");
            }
        }
    }
}

impl App {
    // ------------------------------------------------------------------
    // Agent plumbing (unchanged behavior, kept intact)
    // ------------------------------------------------------------------
    fn poll_agent(&mut self) {
        let Some(rx) = &self.agent.rx else { return };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.agent.pending = false;
                self.agent.status = "agent: request thread died".into();
                return;
            }
        };
        self.agent.pending = false;
        self.agent.rx = None;
        match result {
            Ok(text) => {
                let clean = parse_lenient_json(&text)
                    .unwrap_or_else(|| serde_json::from_str(&text).unwrap_or(serde_json::Value::Null));
                self.agent.status = "agent: ready".into();
                if clean.is_object() {
                    if let Some(c) = clean.get("changes") {
                        if c.is_object() || c.is_array() {
                            self.agent.last_changes = Some(clean.clone());
                        }
                    }
                    let explain = clean
                        .get("explanation")
                        .and_then(|e| e.as_str())
                        .unwrap_or(&text);
                    self.agent.messages.push(("assistant".into(), explain.to_string()));
                } else {
                    self.agent.messages.push(("assistant".into(), text));
                }
            }
            Err(e) => {
                self.agent.status = format!("agent: error: {e}");
                self.agent.messages.push(("assistant".into(), format!("⚠ {e}")));
            }
        }
    }

    fn send_agent(&mut self) {
        if self.agent.pending {
            return;
        }
        let msg = self.agent.input.trim().to_string();
        if msg.is_empty() {
            return;
        }
        self.agent.input.clear();
        self.agent.pending = true;
        self.agent.messages.push(("user".into(), msg.clone()));
        self.agent.last_changes = None;
        let config = self.agent.config.clone();
        let mut msgs: Vec<(String, String)> = self.agent.messages.clone();
        let system = system_prompt(&self.schema, &self.editor.patch);
        msgs.insert(0, ("system".into(), system));
        let (tx, rx) = channel::<AResult<String>>();
        self.agent.rx = Some(rx);
        self.agent.status = "agent: streaming…".into();
        std::thread::spawn(move || {
            let agent = Agent::new(config);
            let res = agent.chat(&msgs);
            let _ = tx.send(res);
        });
    }

    // ------------------------------------------------------------------
    // Generic control helpers (schema + patch bound)
    // ------------------------------------------------------------------
    /// Render a discrete combo for the enum at (section, idx).
    fn combo(&mut self, ui: &mut egui::Ui, section: u8, idx: u8, label: &str) {
        let Some(opts) = self.schema.enum_labels(section, idx).map(|o| o.to_vec()) else {
            return;
        };
        if opts.len() > 34 {
            return; // big enums are knobs
        }
        let cur = self.editor.patch.byte(section as usize, idx as usize).unwrap_or(0);
        let cur_label = self.schema.display(section, idx, cur);
        ui.horizontal(|ui| {
            ui.label(label);
            egui::ComboBox::from_id_salt(("combo", section, idx))
                .selected_text(cur_label)
                .width(220.0)
                .show_ui(ui, |ui| {
                    let mut picked = None;
                    for o in &opts {
                        if ui.selectable_label(cur == o.raw, &o.label).clicked() {
                            picked = Some(o.raw);
                        }
                    }
                    if let Some(v) = picked {
                        self.editor.patch.set_byte(section as usize, idx as usize, v);
                        self.editor.patch.dirty = true;
                    }
                });
        });
    }

    /// Render a rotary knob for (section, idx). Handles Range and big-Enum
    /// (non-linear taper) specs correctly.
    fn knob_param(&mut self, ui: &mut egui::Ui, section: u8, idx: u8, label: &str) {
        let Some(ps) = self.schema.find(section, idx).cloned() else { return };
        let raw = self.editor.patch.byte(section as usize, idx as usize).unwrap_or(0);
        match &ps.value {
            ValueSpec::Range(r) => {
                let (min, max) = (r.data_min, r.data_max);
                let vmin = r.raw_start as f64;
                let vmax = r.raw_end as f64;
                let value = raw_to_data(raw, vmin, vmax, min, max);
                let suffix = r.suffix.clone();
                let spec = KnobSpec {
                    min,
                    max,
                    default: 0.0,
                    fmt: Some(Box::new(move |v| {
                        if suffix.is_empty() {
                            format!("{v:.0}")
                        } else {
                            format!("{v:.0}{suffix}")
                        }
                    })),
                };
                let mut v = value;
                let resp = knob(ui, (section, idx), &mut v, &spec);
                if resp.changed() {
                    let nv = data_to_raw(v, vmin, vmax, min, max).round() as u8;
                    self.editor.patch.set_byte(section as usize, idx as usize, nv);
                    self.editor.patch.dirty = true;
                }
            }
            ValueSpec::Enum(opts) if opts.len() > 34 => {
                let labels: Vec<(u8, String)> = opts.iter().map(|o| (o.raw, o.label.clone())).collect();
                let max_raw = labels.iter().map(|(r, _)| *r).max().unwrap_or(0x7F) as f64;
                let spec = KnobSpec {
                    min: 0.0,
                    max: max_raw,
                    default: 0.0,
                    fmt: Some(Box::new(move |v| {
                        let b = v.round() as u8;
                        labels
                            .iter()
                            .find(|(r, _)| *r == b)
                            .map(|(_, l)| l.clone())
                            .unwrap_or_else(|| format!("{b}"))
                    })),
                };
                let mut v = raw as f64;
                let resp = knob(ui, (section, idx), &mut v, &spec);
                if resp.changed() {
                    self.editor.patch.set_byte(section as usize, idx as usize, v.round() as u8);
                    self.editor.patch.dirty = true;
                }
            }
            ValueSpec::Data(_) => {
                let spec = KnobSpec {
                    min: 0.0,
                    max: 0x7F as f64,
                    default: 0.0,
                    fmt: Some(Box::new(|v| format!("{v:.0}"))),
                };
                let mut v = raw as f64;
                let resp = knob(ui, (section, idx), &mut v, &spec);
                if resp.changed() {
                    self.editor.patch.set_byte(section as usize, idx as usize, v.round() as u8);
                    self.editor.patch.dirty = true;
                }
            }
            _ => {}
        }
        if !label.is_empty() {
            ui.label(egui::RichText::new(label).size(10.0).color(DIM));
        }
    }

    /// Render one parameter with the widget appropriate to its spec:
    /// small enums as ComboBox, large enums and ranges as knobs.
    fn any_param(&mut self, ui: &mut egui::Ui, section: u8, idx: u8) {
        let enum_len = self
            .schema
            .enum_labels(section, idx)
            .map(|v| v.len())
            .unwrap_or(0);
        if enum_len > 34 {
            self.knob_param(ui, section, idx, "");
        } else if enum_len > 0 {
            self.combo(ui, section, idx, "");
        } else {
            self.knob_param(ui, section, idx, "");
        }
    }

    /// Vertical fader for EQ bands (-12..+12 dB over raw 0..30).
    fn vfader(&mut self, ui: &mut egui::Ui, section: u8, idx: u8, label: &str, raw_min: u8, raw_max: u8, data_min: f64, data_max: f64) {
        let raw = self.editor.patch.byte(section as usize, idx as usize).unwrap_or(0);
        let mut pos = raw_to_data(raw, raw_min as f64, raw_max as f64, data_min, data_max);
        let resp = fader(
            ui,
            (section, idx),
            &mut pos,
            data_min, data_max, 0.0,
        );
        if resp.changed() {
            let nv = data_to_raw(pos, raw_min as f64, raw_max as f64, data_min, data_max).round() as u8;
            self.editor.patch.set_byte(section as usize, idx as usize, nv);
            self.editor.patch.dirty = true;
        }
        ui.label(format!("{label}  {:+.1}", pos));
    }

    // ------------------------------------------------------------------
    // Tabs
    // ------------------------------------------------------------------
    fn amp_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(420.0);
                    // Amp type card
                    card(ui, "PREAMP", |ui| {
                        self.combo(ui, 0x00, 0x20, "Amp");
                        self.combo(ui, 0x00, 0x21, "Type");
                        ui.horizontal(|ui| {
                            self.knob_param(ui, 0x00, 0x22, "GAIN");
                            self.knob_param(ui, 0x00, 0x24, "BASS");
                            self.knob_param(ui, 0x00, 0x25, "MID");
                            self.knob_param(ui, 0x00, 0x26, "TREBLE");
                            self.knob_param(ui, 0x00, 0x27, "PRESENCE");
                            self.knob_param(ui, 0x00, 0x28, "VOLUME");
                        });
                        ui.separator();
                        self.combo(ui, 0x00, 0x29, "Bright");
                        self.combo(ui, 0x00, 0x2A, "Gain Switch");
                        self.combo(ui, 0x00, 0x2B, "Solo Switch");
                    });
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_max_width(420.0);
                    card(ui, "BOOSTER", |ui| {
                        self.combo(ui, 0x00, 0x10, "On/Off");
                        self.combo(ui, 0x00, 0x11, "Type");
                        ui.horizontal(|ui| {
                            self.knob_param(ui, 0x00, 0x12, "GAIN");
                            self.knob_param(ui, 0x00, 0x13, "LOW");
                            self.knob_param(ui, 0x00, 0x14, "HIGH");
                            self.knob_param(ui, 0x00, 0x17, "LEVEL");
                        });
                    });
                });
            });
        });
    }

    /// One parametric equalizer card (PEQ1 at base 0x40, PEQ2 at 0x60).
    fn peq_card(&mut self, ui: &mut egui::Ui, title: &str, base: u8) {
        card(ui, title, |ui| {
            self.combo(ui, 0x00, base, "On/Off");
            self.combo(ui, 0x00, base + 1, "Selection");
            self.combo(ui, 0x00, base + 2, "Low Cut");
            ui.horizontal(|ui| {
                self.combo(ui, 0x00, base + 4, "Lo Mid Freq");
                self.combo(ui, 0x00, base + 5, "Q");
                self.knob_param(ui, 0x00, base + 6, "");
            });
            ui.horizontal(|ui| {
                self.combo(ui, 0x00, base + 7, "Hi Mid Freq");
                self.combo(ui, 0x00, base + 8, "Q");
                self.knob_param(ui, 0x00, base + 9, "");
            });
            self.combo(ui, 0x00, base + 0x0B, "High Cut");
            ui.horizontal(|ui| {
                self.knob_param(ui, 0x00, base + 3, "LOW GAIN");
                self.knob_param(ui, 0x00, base + 0x0A, "HIGH GAIN");
                self.knob_param(ui, 0x00, base + 0x0C, "LEVEL");
            });
        });
    }

    /// 10-band graphic EQ vertical faders (GEQ1 at 0x4D, GEQ2 at 0x6D).
    fn geq_card(&mut self, ui: &mut egui::Ui, title: &str, base: u8) {
        card(ui, title, |ui| {
            ui.horizontal(|ui| {
                let bands = ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
                for (i, b) in bands.iter().enumerate() {
                    ui.vertical(|ui| {
                        self.vfader(ui, 0x00, base + i as u8, b, 0x00, 0x1E, -12.0, 12.0);
                    });
                }
            });
        });
    }

    fn eq_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                self.peq_card(ui, "PARAMETRIC EQ 1", 0x40);
                ui.add_space(12.0);
                self.peq_card(ui, "PARAMETRIC EQ 2", 0x60);
            });
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                self.geq_card(ui, "GRAPHIC EQ 1  (Hz)", 0x4D);
                ui.add_space(12.0);
                self.geq_card(ui, "GRAPHIC EQ 2  (Hz)", 0x6D);
            });
        });
    }

    fn fx_page(&mut self, ui: &mut egui::Ui, section_main: u8, section_ext: u8, title: &str) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let selected_type = self
                .schema
                .display(
                    section_main,
                    0x01,
                    self.editor.patch.byte(section_main as usize, 0x01).unwrap_or(0),
                )
                .trim()
                .to_string();
            card(ui, &format!("{title} — MAIN"), |ui| {
                self.combo(ui, section_main, 0x00, "On/Off");
                self.combo(ui, section_main, 0x01, "Type");
                // all range knobs from 0x02..0x20
                ui.horizontal_wrapped(|ui| {
                    for idx in 0x02u8..=0x1F {
                        let odd = idx % 3 == 0;
                        ui.vertical(|ui| {
                            let label = self.schema.find(section_main, idx)
                                .map(|p| p.label.clone())
                                .unwrap_or_default();
                            self.any_param(ui, section_main, idx);
                            ui.label(label.trim_end_matches(':'));
                        });
                        if odd { ui.add_space(4.0); }
                    }
                });
            });
            ui.add_space(12.0);
            card(ui, &format!("{title} — EXTENDED ({selected_type})"), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for idx in 0x00u8..=0x60 {
                        if self.schema.find(section_ext, idx).is_none() {
                            continue;
                        }
                        ui.vertical(|ui| {
                            self.any_param(ui, section_ext, idx);
                            ui.label(format!("{:02X}", idx));
                        });
                    }
                });
            });
        });
    }

    fn delay_reverb_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            card(ui, "DELAY 1", |ui| {
                self.combo(ui, 0x05, 0x00, "On/Off");
                self.combo(ui, 0x05, 0x01, "Type");
                ui.horizontal_wrapped(|ui| {
                    for idx in 0x02u8..=0x1F {
                        ui.vertical(|ui| {
                            self.any_param(ui, 0x05, idx);
                            ui.label(format!("{idx:02X}"));
                        });
                    }
                });
            });
            ui.add_space(12.0);
            card(ui, "DELAY 2", |ui| {
                self.combo(ui, 0x05, 0x20, "On/Off");
                self.combo(ui, 0x05, 0x21, "Type");
                ui.horizontal_wrapped(|ui| {
                    for idx in 0x22u8..=0x39 {
                        ui.vertical(|ui| {
                            self.any_param(ui, 0x05, idx);
                            ui.label(format!("{idx:02X}"));
                        });
                    }
                });
            });
            ui.add_space(12.0);
            card(ui, "REVERB", |ui| {
                self.combo(ui, 0x05, 0x40, "On/Off");
                self.combo(ui, 0x05, 0x41, "Type");
                ui.horizontal_wrapped(|ui| {
                    for idx in 0x42u8..=0x4B {
                        ui.vertical(|ui| {
                            self.any_param(ui, 0x05, idx);
                            ui.label(format!("{idx:02X}"));
                        });
                    }
                });
            });
        });
    }

    fn output_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            card(ui, "OUTPUT / PANEL", |ui| {
                self.combo(ui, 0x06, 0x00, "Master");
                self.combo(ui, 0x06, 0x01, "Output Select");
                self.combo(ui, 0x06, 0x02, "Cab Select");
                ui.horizontal_wrapped(|ui| {
                    for idx in 0x04u8..=0x30 {
                        ui.vertical(|ui| {
                            self.any_param(ui, 0x06, idx);
                            ui.label(format!("{idx:02X}"));
                        });
                    }
                });
            });
        });
    }

    fn assign_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let names = [
                (0x07usize, "FS Assign"),
                (0x08, "EXP Pedal Assign"),
                (0x09, "GAFC EXP1"),
                (0x0A, "GAFC EXP2"),
                (0x0B, "GAFC EXP3"),
                (0x0C, "GAFC-EX EXP1"),
                (0x0D, "GAFC-EX EXP2"),
                (0x0E, "GAFC-EX EXP3"),
                (0x0F, "Panel / Solo"),
            ];
            for (sec, title) in names {
                card(ui, title, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for idx in 0x00u8..=0x28 {
                            let Some(ps) = self.schema.find(sec as u8, idx) else { continue };
                            if matches!(ps.value, ValueSpec::Enum(_)) && idx <= 0x21 {
                                self.combo(ui, sec as u8, idx, "");
                            }
                        }
                    });
                });
            }
        });
    }

    fn chat_center(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.agent.messages.is_empty() {
                    ui.label("Ask the agent to replicate a band/song/style…");
                }
                for (role, content) in &self.agent.messages {
                    let is_user = role == "user";
                    ui.horizontal(|ui| {
                        if is_user { ui.add_space(80.0); }
                        egui::Frame::none()
                            .fill(if is_user { egui::Color32::from_rgb(0x1D, 0x33, 0x4A) } else { egui::Color32::from_rgb(0x1E, 0x2E, 0x24) })
                            .corner_radius(8.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.set_max_width(ui.available_width() - 40.0);
                                let (who, color) = if is_user { ("You", ACCENT) } else { ("Agent", egui::Color32::from_rgb(0x86, 0xCF, 0x7C)) };
                                ui.colored_label(color, who);
                                ui.label(content);
                            });
                        if !is_user { ui.add_space(80.0); }
                    });
                    ui.add_space(4.0);
                }
            });
        // input row
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.agent.input)
                .hint_text("Ask about a band/song/style… (e.g. \"Metallica Master of Puppets lead tone\")")
                .desired_width(ui.available_width() - 70.0);
            let resp = ui.add(edit);
            let send = ui.add_enabled(!self.agent.pending, egui::Button::new("Send"));
            if send.clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                self.send_agent();
            }
        });
        if self.agent.pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("thinking…");
            });
        } else if self.agent.last_changes.is_some() {
            ui.horizontal(|ui| {
                if ui.button("▶ Apply to amp").clicked() {
                    self.apply_agent_changes();
                }
                if ui.button("⟲ Dismiss").clicked() {
                    self.agent.last_changes = None;
                }
            });
        }
        ui.label(&self.agent.status);
    }
}

// ---------------------------------------------------------------------
// Layout widgets
// ---------------------------------------------------------------------
fn card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD)
        .corner_radius(10.0)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(0x35, 0x35, 0x42)))
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(260.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.label(egui::RichText::new(title).strong().color(ACCENT).size(13.0));
                ui.add_space(6.0);
                add_contents(ui);
            });
        });
}

/// Vertical fader (drag up/down) with filled track and center notch.
fn fader(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, value: &mut f64, min: f64, max: f64, default: f64) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(28.0, 150.0), egui::Sense::drag());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect.expand(2.0));
        let track = egui::Rect::from_center_size(rect.center(), egui::vec2(6.0, rect.height() - 14.0));
        painter.rect_filled(track, 3.0, egui::Color32::from_rgb(0x2C, 0x2C, 0x38));
        let t = ((*value - min) / (max - min)).clamp(0.0, 1.0) as f32;
        let y = track.bottom() - t * track.height();
        let mut active = track;
        active.max.y = y;
        painter.rect_filled(active, 3.0, ACCENT);
        // center line
        let cy = track.bottom() - ((0.0 - min) / (max - min)).clamp(0.0, 1.0) as f32 * track.height();
        painter.line_segment(
            [egui::pos2(track.left(), cy), egui::pos2(track.right(), cy)],
            egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
        );
        // thumb
        painter.circle_filled(egui::pos2(rect.center().x, y), 6.0, egui::Color32::from_rgb(0xE8, 0xE8, 0xF0));
    }
    let mut changed = false;
    if resp.dragged() {
        let dy = resp.drag_delta().y;
        let step = if ui.input(|i| i.modifiers.shift) { (max - min) / 1000.0 } else { (max - min) / 200.0 };
        let nv = *value - dy as f64 * step;
        *value = nv.clamp(min, max);
        changed = true;
    }
    if resp.double_clicked() {
        *value = default;
        changed = true;
    }
    if changed {
        resp.mark_changed();
    }
    let _ = id_salt;
    resp
}

fn raw_to_data(raw: u8, raw_min: f64, raw_max: f64, data_min: f64, data_max: f64) -> f64 {
    if raw_max <= raw_min { return data_min; }
    ((raw as f64 - raw_min) / (raw_max - raw_min)) * (data_max - data_min) + data_min
}

fn data_to_raw(v: f64, raw_min: f64, raw_max: f64, data_min: f64, data_max: f64) -> f64 {
    if data_max <= data_min { return raw_min; }
    ((v - data_min) / (data_max - data_min)) * (raw_max - raw_min) + raw_min
}

// ---------------------------------------------------------------------
// eframe App
// ---------------------------------------------------------------------
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_midi();
        self.poll_agent();

        // ---- top bar ----
        egui::TopBottomPanel::top("top").frame(
            egui::Frame::none().fill(egui::Color32::from_rgb(0x10, 0x10, 0x14)).inner_margin(10.0),
        ).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("KATANA MK2").strong().size(18.0).color(ACCENT));
                ui.label(egui::RichText::new("EDITOR").size(18.0).color(DIM));
                ui.separator();
                match &self.connect {
                    ConnectState::Idle => {
                        ui.colored_label(DIM, "● Not connected");
                    }
                    ConnectState::Connected => {
                        ui.colored_label(egui::Color32::from_rgb(0x6F, 0xD9, 0x6F), "● Connected");
                    }
                    ConnectState::Failed(e) => {
                        ui.colored_label(egui::Color32::from_rgb(0xE0, 0x6C, 0x6C), format!("● {e}"));
                    }
                }
                ui.separator();
                // port selectors + connect
                let ins = self.ports.0.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
                let outs = self.ports.1.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
                let in_sel = self.sel_in
                    .map(|i| ins.get(i).cloned().unwrap_or_else(|| "Select MIDI in".into()))
                    .unwrap_or_else(|| "Select MIDI in".into());
                let out_sel = self.sel_out
                    .map(|i| outs.get(i).cloned().unwrap_or_else(|| "Select MIDI out".into()))
                    .unwrap_or_else(|| "Select MIDI out".into());
                egui::ComboBox::from_id_salt("midi_in")
                    .selected_text(in_sel)
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for (i, n) in ins.iter().enumerate() {
                            if ui.selectable_label(self.sel_in == Some(i), n).clicked() {
                                self.sel_in = Some(i);
                            }
                        }
                    });
                egui::ComboBox::from_id_salt("midi_out")
                    .selected_text(out_sel)
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for (i, n) in outs.iter().enumerate() {
                            if ui.selectable_label(self.sel_out == Some(i), n).clicked() {
                                self.sel_out = Some(i);
                            }
                        }
                    });
                if ui.button("Connect").clicked() {
                    if let (Some(i), Some(o)) = (self.sel_in, self.sel_out) {
                        let iname = ins.get(i).cloned().unwrap_or_default();
                        let oname = outs.get(o).cloned().unwrap_or_default();
                        self.bus.set_device(iname, oname);
                        self.editor.status = "connecting…".into();
                    }
                }
                if ui.button("⟳").on_hover_text("Refresh ports").clicked() {
                    self.ports = MidiBus::list_ports().unwrap_or_default();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let name = String::from_utf8_lossy(&self.editor.patch.name_bytes()).trim_end().to_string();
                    ui.label(egui::RichText::new(name).strong().size(15.0).color(TEXT));
                    ui.label(egui::RichText::new("Patch").size(12.0).color(DIM));
                    ui.separator();
                    let online = matches!(self.connect, ConnectState::Connected);
                    let (led, label) = if online {
                        (egui::Color32::from_rgb(0x6F, 0xD9, 0x6F), "Amp Online")
                    } else {
                        (egui::Color32::from_rgb(0xE0, 0x6C, 0x6C), "Amp Offline")
                    };
                    ui.colored_label(led, format!("● {label}"));
                });
            });
        });

        // ---- left transport panel ----
        egui::SidePanel::left("transport")
            .resizable(false)
            .default_width(230.0)
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(0x18, 0x18, 0x1E)).inner_margin(10.0))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("TRANSPORT").strong().size(12.0).color(ACCENT));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Bank");
                    ui.add(egui::DragValue::new(&mut self.editor.bank).range(1..=50));
                });
                ui.horizontal(|ui| {
                    ui.label("Patch");
                    ui.add(egui::DragValue::new(&mut self.editor.slot).range(1..=9));
                });
                ui.horizontal(|ui| {
                    if ui.button("⬇ Read Patch").clicked() {
                        self.editor.status = format!("reading bank {} patch {}…", self.editor.bank, self.editor.slot);
                        self.editor.rx_frames.clear();
                        self.editor.rx_buf.clear();
                        let _ = self.amp.request_patch(self.editor.bank, self.editor.slot);
                    }
                    if ui.button("⬆ Sync to Amp").clicked() {
                        self.editor.status = "syncing patch to amp temp buffer…".into();
                        self.amp.push_patch(&self.editor.patch);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("💾 Write Slot").clicked() {
                        self.amp.write_slot(self.editor.slot);
                        self.editor.status = format!("patch written to slot {}", self.editor.slot);
                    }
                    ui.add(egui::DragValue::new(&mut self.editor.slot).range(1..=9));
                });
                ui.separator();
                ui.label(egui::RichText::new("AGENT").strong().size(12.0).color(ACCENT));
                ui.label(&self.agent.status);
                let edit = egui::TextEdit::singleline(&mut self.agent.input)
                    .hint_text("Tone request…")
                    .desired_width(ui.available_width() - 62.0);
                let resp = ui.add(edit);
                let send = ui.add_enabled(!self.agent.pending, egui::Button::new("Send"));
                if send.clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.send_agent();
                }
                if self.agent.pending {
                    ui.horizontal(|ui| { ui.spinner(); ui.label("thinking…"); });
                }
                if let Some(_) = self.agent.last_changes.clone() {
                    if ui.button("▶ Apply to amp").clicked() {
                        self.apply_agent_changes();
                    }
                }
            });

        // ---- bottom status ----
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(0x10, 0x10, 0x14)).inner_margin(6.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(DIM, &self.editor.status);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.editor.patch.dirty {
                            ui.colored_label(egui::Color32::from_rgb(0xE0, 0xC0, 0x60), "● unsaved");
                        }
                        if self.editor.rx_frames.len() > 0 {
                            ui.colored_label(DIM, format!("rx {}B", self.editor.rx_frames.len()));
                        }
                    });
                });
            });

        // ---- central ----
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tabs = [
                    (Page::Amp, "Amp"),
                    (Page::EQ, "EQ"),
                    (Page::FX1, "FX1"),
                    (Page::FX2, "FX2"),
                    (Page::DelayReverb, "Dly/Rvb"),
                    (Page::Output, "Output"),
                    (Page::Assign, "Assign"),
                    (Page::Chat, "Agent Chat"),
                ];
                for (p, label) in tabs {
                    let selected = self.editor.page == p;
                    let resp = ui.add(
                        egui::Button::new(
                            egui::RichText::new(label)
                                .color(if selected { ACCENT } else { DIM })
                                .strong(),
                        )
                        .fill(if selected { egui::Color32::from_rgb(0x24, 0x2E, 0x3C) } else { CARD })
                    );
                    if resp.clicked() {
                        self.editor.page = p;
                    }
                }
            });
            ui.separator();
            match self.editor.page {
                Page::Amp => self.amp_page(ui),
                Page::EQ => self.eq_page(ui),
                Page::FX1 => self.fx_page(ui, 0x01, 0x02, "FX1 (Modulation)"),
                Page::FX2 => self.fx_page(ui, 0x03, 0x04, "FX2 (Modulation)"),
                Page::DelayReverb => self.delay_reverb_page(ui),
                Page::Output => self.output_page(ui),
                Page::Assign => self.assign_page(ui),
                Page::Chat => self.chat_center(ui),
            }
        });
    }
}
