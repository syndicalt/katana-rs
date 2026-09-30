//! Tauri backend for Katana Editor.
//!
//! Bridges the frontend (React) to the `katana_rs_proto` protocol core:
//! MIDI bus, amp patch transfer, live-follow events, and the LLM agent.

use katana_rs_proto::agent::{
    apply_sparse_changes, apply_tsl_json, parse_lenient_json, system_prompt, Agent, AgentConfig,
    ChatMessage, Provider,
};
use katana_rs_proto::amp::AmpApi;
use katana_rs_proto::live;
use katana_rs_proto::midi::{BusEvent, MidiBus, PortInfo};
use katana_rs_proto::patch::Patch;
use katana_rs_proto::protocol::{self, Frame};
use katana_rs_proto::schema::{Schema, ValueSpec};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Listener, Manager};

// ---------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------

struct AppState {
    bus: MidiBus,
    amp: AmpApi,
    schema: Schema,
    patch: Mutex<Patch>,
    agent: Mutex<AgentSession>,
    pending_live: Mutex<Vec<Frame>>,
    rx_buf: Mutex<Vec<u8>>,
    rx_frames: Mutex<Vec<u8>>,
    bank: Mutex<usize>,
    slot: Mutex<usize>,
}

struct AgentSession {
    config: AgentConfig,
    messages: Vec<ChatMessage>,
    rx: Option<Receiver<anyhow::Result<String>>>,
    pending: bool,
    last_changes: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------

#[derive(Serialize)]
struct PortsDto {
    inputs: Vec<PortDto>,
    outputs: Vec<PortDto>,
}

#[derive(Serialize, Clone)]
struct PortDto {
    name: String,
    is_katanaport: bool,
}

#[tauri::command]
fn list_ports() -> Result<PortsDto, String> {
    let (ins, outs) = MidiBus::list_ports().map_err(|e| e.to_string())?;
    Ok(PortsDto {
        inputs: ins.into_iter().map(|p| PortDto { name: p.name.clone(), is_katanaport: p.is_katanaport }).collect(),
        outputs: outs.into_iter().map(|p| PortDto { name: p.name.clone(), is_katanaport: p.is_katanaport }).collect(),
    })
}

#[tauri::command]
fn connect(state: tauri::State<AppState>, in_name: String, out_name: String) {
    state.bus.set_device(in_name, out_name);
}

#[tauri::command]
fn read_patch(state: tauri::State<AppState>) -> Result<(), String> {
    let bank = *state.bank.lock().unwrap();
    let slot = *state.slot.lock().unwrap();
    state.amp.request_patch(bank, slot).map_err(|e| e.to_string())
}

#[tauri::command]
fn sync_patch(state: tauri::State<AppState>) {
    let patch = state.patch.lock().unwrap();
    state.amp.push_patch(&patch);
}

#[tauri::command]
fn write_slot(state: tauri::State<AppState>) {
    let slot = *state.slot.lock().unwrap();
    state.amp.write_slot(slot);
}

#[tauri::command]
fn set_param(state: tauri::State<AppState>, section: u8, index: u8, value: u8) {
    let mut patch = state.patch.lock().unwrap();
    patch.set_byte(section as usize, index as usize, value);
    patch.dirty = true;
}

#[derive(Serialize)]
struct ParamDto {
    section: u8,
    index: u8,
    label: String,
    kind: String,
    enum_values: Option<Vec<EnumOptDto>>,
    range: Option<RangeDto>,
    raw: u8,
    display: String,
}

#[derive(Serialize)]
struct EnumOptDto {
    raw: u8,
    label: String,
}

#[derive(Serialize)]
struct RangeDto {
    raw_min: u8,
    raw_max: u8,
    data_min: f64,
    data_max: f64,
    suffix: String,
}

#[tauri::command]
fn get_patch_params(state: tauri::State<AppState>) -> Result<Vec<ParamDto>, String> {
    let patch = state.patch.lock().unwrap();
    let mut out = Vec::new();
    for a1 in 0u8..=0x0F {
        let Some(group) = state.schema.group(a1) else { continue };
        for p in &group.params {
            let raw = patch.byte(a1 as usize, p.idx as usize).unwrap_or(0);
            let (kind, enum_values, range) = match &p.value {
                ValueSpec::Enum(opts) => (
                    "enum",
                    Some(opts.iter().map(|o| EnumOptDto { raw: o.raw, label: o.label.clone() }).collect()),
                    None,
                ),
                ValueSpec::Range(r) => (
                    "range",
                    None,
                    Some(RangeDto {
                        raw_min: r.raw_start,
                        raw_max: r.raw_end,
                        data_min: r.data_min,
                        data_max: r.data_max,
                        suffix: r.suffix.clone(),
                    }),
                ),
                ValueSpec::Data(_) => ("data", None, None),
            };
            out.push(ParamDto {
                section: a1,
                index: p.idx,
                label: if p.label.is_empty() { format!("{:02X}", p.idx) } else { p.label.clone() },
                kind: kind.to_string(),
                enum_values,
                range,
                raw,
                display: state.schema.display(a1, p.idx, raw),
            });
        }
    }
    Ok(out)
}

#[tauri::command]
fn get_patch_name(state: tauri::State<AppState>) -> Result<String, String> {
    let patch = state.patch.lock().unwrap();
    Ok(String::from_utf8_lossy(&patch.name_bytes()).trim_end().to_string())
}


#[tauri::command]
fn set_chain(state: tauri::State<AppState>, chain: Vec<u8>) -> Result<(), String> {
    if chain.len() != 20 {
        return Err(format!("chain must be 20 bytes, got {}", chain.len()));
    }
    let mut patch = state.patch.lock().unwrap();
    for (i, b) in chain.iter().enumerate() {
        patch.set_byte(6, i, *b); // section 0x06 (panel), idx i
    }
    patch.dirty = true;
    // Push the reordered chain to the amp temp buffer (whole section write
    // via dataChange is handled by push_patch; here just update local state.)
    Ok(())
}

#[tauri::command]
fn get_chain(state: tauri::State<AppState>) -> Result<Vec<u8>, String> {
    let patch = state.patch.lock().unwrap();
    Ok((0..20).map(|i| patch.byte(6, i).unwrap_or(0)).collect())
}

// Chat -----------------------------------------------------------------

#[derive(Serialize)]
struct ChatReply {
    text: String,
    has_changes: bool,
}

#[tauri::command]
fn agent_send(state: tauri::State<AppState>, message: String) -> Result<(), String> {
    let mut agent = state.agent.lock().unwrap();
    if agent.pending {
        return Ok(());
    }
    agent.pending = true;
    agent.messages.push(("user".to_string(), message.clone()));
    let config = agent.config.clone();
    let mut msgs = agent.messages.clone();
    let system = system_prompt(&state.schema, &state.patch.lock().unwrap());
    msgs.insert(0, ("system".to_string(), system));
    let (tx, rx) = channel();
    agent.rx = Some(rx);
    std::thread::spawn(move || {
        let a = Agent::new(config);
        let res = a.chat(&msgs);
        let _ = tx.send(res);
    });
    Ok(())
}

#[tauri::command]
fn agent_poll(state: tauri::State<AppState>, app: AppHandle) -> Result<Option<ChatReply>, String> {
    let mut agent = state.agent.lock().unwrap();
    let Some(rx) = &agent.rx else { return Ok(None) };
    let result = match rx.try_recv() {
        Ok(r) => r,
        Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(None),
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            agent.pending = false;
            return Err("agent thread died".into());
        }
    };
    agent.pending = false;
    match result {
        Ok(text) => {
            let clean = parse_lenient_json(&text)
                .unwrap_or_else(|| serde_json::from_str(&text).unwrap_or(serde_json::Value::Null));
            let mut has_changes = false;
            if let Some(v) = clean.get("changes") {
                has_changes = v.is_object() || v.is_array();
                agent.last_changes = Some(v.clone());
            }
            let explain = clean.get("explanation").and_then(|e| e.as_str()).unwrap_or(&text);
            agent.messages.push(("assistant".to_string(), explain.to_string()));
            let _ = app.emit("agent-reply", &explain);
            Ok(Some(ChatReply { text: explain.to_string(), has_changes }))
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn agent_apply(state: tauri::State<AppState>) -> Result<usize, String> {
    // Last changes must have been passed in; simplest: re-run parse on last
    // assistant text stored in state? For now assume apply_tsl_json over the
    // most recent reply stored in session.
    let changes = {
        let agent = state.agent.lock().unwrap();
        agent.last_changes.clone().ok_or_else(|| "no agent changes to apply".to_string())?
    };
    let mut patch = state.patch.lock().unwrap();
    let n = if changes.is_array() {
        apply_sparse_changes(&mut patch, &changes).map_err(|e| e.to_string())?
    } else if changes.is_object() {
        apply_tsl_json(&mut patch, &changes).map_err(|e| e.to_string())?
    } else {
        apply_tsl_json(&mut patch, &changes).map_err(|e| e.to_string())?
    };
    patch.dirty = true;
    Ok(n)
}

// ---------------------------------------------------------------------
// MIDI event pump: poll bus events, apply live frames, emit to frontend
// ---------------------------------------------------------------------

pub fn pump(app: &AppHandle, state: &AppState) {
    while let Some(ev) = state.bus.try_recv() {
        match ev {
            BusEvent::Connected => {
                state.amp.enable_verbose();
                let _ = state.amp.request_patch(*state.bank.lock().unwrap(), *state.slot.lock().unwrap());
                let _ = app.emit("amp-connected", ());
            }
            BusEvent::Disconnected(reason) => {
                let _ = app.emit("amp-disconnected", &reason);
            }
            BusEvent::Received(data) => {
                let mut buf = state.rx_buf.lock().unwrap();
                buf.extend_from_slice(&data);
                assemble(state, app, &mut buf);
            }
        }
    }
}

fn assemble(state: &AppState, app: &AppHandle, buf: &mut Vec<u8>) {
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == 0xF0 {
            if let Some(pos) = buf[i..].iter().position(|&b| b == 0xF7) {
                let end = i + pos + 1;
                let frame_bytes = buf[i..end].to_vec();
                buf.drain(..end);
                if let Some((f, _)) = protocol::Frame::parse(&frame_bytes) {
                    if live::is_live_frame(&f) && f.data.len() <= 32 {
                        let mut frames = state.rx_frames.lock().unwrap();
                        if !frames.is_empty() && frames.len() < protocol::PATCH_REPLY_SIZE {
                            state.pending_live.lock().unwrap().push(f);
                        } else {
                            let mut patch = state.patch.lock().unwrap();
                            let n = live::apply_live(&mut patch, &f);
                            let _ = app.emit("amp-param-changed", serde_json::json!({
                                "section": f.address[2],
                                "index": f.address[3],
                                "bytes": f.data,
                            }));
                            let _ = n;
                        }
                        continue;
                    }
                }
                state.rx_frames.lock().unwrap().extend_from_slice(&frame_bytes);
                continue;
            }
        }
        i += 1;
    }
    let mut frames = state.rx_frames.lock().unwrap();
    if frames.len() >= protocol::PATCH_REPLY_SIZE {
        let tpl = include_bytes!("../../assets/default.syx");
        let reply = std::mem::take(&mut *frames);
        let merged = Patch::from_reply(tpl, &reply);
        let mut patch = state.patch.lock().unwrap();
        patch.frames = merged.frames;
        for f in state.pending_live.lock().unwrap().drain(..) {
            live::apply_live(&mut patch, &f);
        }
        patch.dirty = false;
        let _ = app.emit("patch-loaded", ());
    }
}

// ---------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let schema = Schema::load(include_str!("../../assets/midi.xml")).expect("midi.xml");
    let tpl = include_bytes!("../../assets/default.syx");
    let bus = MidiBus::spawn();
    let amp = AmpApi::new(&bus);
    let hook = tauri_plugin_log::Builder::default().build();
    tauri::Builder::default()
        .plugin(hook)
        .manage(AppState {
            bus,
            amp,
            schema,
            patch: Mutex::new(Patch::from_template(tpl, "New Patch".into())),
            agent: Mutex::new(AgentSession {
                config: AgentConfig::load(),
                messages: Vec::new(),
                rx: None,
                pending: false,
                last_changes: None,
            }),
            pending_live: Mutex::new(Vec::new()),
            rx_buf: Mutex::new(Vec::new()),
            rx_frames: Mutex::new(Vec::new()),
            bank: Mutex::new(1),
            slot: Mutex::new(1),
        })
        .invoke_handler(tauri::generate_handler![
            list_ports,
            connect,
            read_patch,
            sync_patch,
            write_slot,
            set_param,
            set_chain,
            get_chain,
            get_patch_params,
            get_patch_name,
            agent_send,
            agent_poll,
            agent_apply,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let app2 = handle.clone();
            std::thread::spawn(move || {
                loop {
                    let state = app2.state::<AppState>(); {
                        let _ = &state;
                        pump(&handle, &state);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
