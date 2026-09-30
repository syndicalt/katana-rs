//! ALSA MIDI transport: device enumeration, send/receive threading.
//!
//! Thread model: app UI thread ⇄ `MidiBus` (mpsc channel) ⇄ IO thread.
//! The IO thread owns the midir connections and blocks on receive.

use anyhow::Result;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use parking_lot::Mutex;
use std::thread::JoinHandle;

/// Message from app to IO thread.
pub enum BusMsg {
    Send(Vec<u8>),
    SetDevice { in_name: String, out_name: String },
    Refresh,
}

/// Message from IO thread to app.
pub enum BusEvent {
    Connected,
    Disconnected(String),
    Received(Vec<u8>),
}

/// MIDI port descriptor.
#[derive(Clone, Debug)]
pub struct PortInfo {
    pub name: String,
    pub is_katanaport: bool,
}

/// Bidirectional MIDI bus.
pub struct MidiBus {
    tx: Sender<BusMsg>,
    /// Receiver is not Sync; wrap to make MidiBus usable from Tauri state.
    rx: Mutex<Receiver<BusEvent>>,
    status: Arc<Mutex<Status>>,
    _thread: Option<JoinHandle<()>>,
}

#[derive(Default, Clone)]
struct Status {
    connected: bool,
    in_port: String,
    out_port: String,
}

impl MidiBus {
    pub fn spawn() -> MidiBus {
        let (tx, bus_rx) = channel::<BusMsg>();
        let (ev_tx, ev_rx) = channel::<BusEvent>();
        let status = Arc::new(Mutex::new(Status::default()));
        let status2 = status.clone();
        let thread = std::thread::spawn(move || {
            io_loop(bus_rx, ev_tx, status2);
        });
        MidiBus { tx, rx: Mutex::new(ev_rx), status, _thread: Some(thread) }
    }

    pub fn send(&self, data: Vec<u8>) {
        let _ = self.tx.send(BusMsg::Send(data));
    }

    pub fn set_device(&self, in_name: String, out_name: String) {
        let _ = self.tx.send(BusMsg::SetDevice { in_name, out_name });
    }

    pub fn refresh(&self) {
        let _ = self.tx.send(BusMsg::Refresh);
    }

    pub fn try_recv(&self) -> Option<BusEvent> {
        self.rx.lock().try_recv().ok()
    }

    pub fn is_connected(&self) -> bool {
        self.status.lock().connected
    }

    pub fn port_names(&self) -> (String, String) {
        let s = self.status.lock();
        (s.in_port.clone(), s.out_port.clone())
    }

    /// Clone of the message sender (for [`crate::amp::AmpApi`]).
    pub fn sender(&self) -> Sender<BusMsg> {
        self.tx.clone()
    }


    /// Enumerate all ports.
    pub fn list_ports() -> Result<(Vec<PortInfo>, Vec<PortInfo>)> {
        let mi = MidiInput::new("katana-rs")?;
        let mo = MidiOutput::new("katana-rs")?;
        let ins: Vec<PortInfo> = mi.ports().iter()
            .map(|p| PortInfo {
                name: mi.port_name(p).unwrap_or_default(),
                is_katanaport: mi.port_name(p).map(|n| n.to_uppercase().contains("KATANA")).unwrap_or(false),
            })
            .collect();
        let outs: Vec<PortInfo> = mo.ports().iter()
            .map(|p| PortInfo {
                name: mo.port_name(p).unwrap_or_default(),
                is_katanaport: mo.port_name(p).map(|n| n.to_uppercase().contains("KATANA")).unwrap_or(false),
            })
            .collect();
        Ok((ins, outs))
    }
}

fn io_loop(rx: Receiver<BusMsg>, ev_tx: Sender<BusEvent>, status: Arc<Mutex<Status>>) {
    // Keep the input connection alive so midir keeps delivering events.
    let mut _input: Option<MidiInputConnection<()>> = None;
    let mut output: Option<MidiOutputConnection> = None;

    while let Ok(msg) = rx.recv() {
        match msg {
            BusMsg::Refresh => {
                let _ = ev_tx.send(BusEvent::Connected);
            }
            BusMsg::SetDevice { in_name, out_name } => {
                // midir requires the callback to be 'static: clone the sender
                // for it. Built per-connection; connect consumes it.
                let cb_tx = ev_tx.clone();
                let input_cb = move |_ts: u64, msg: &[u8], _: &mut ()| {
                    // A sysex frame may arrive split across callbacks; assemble until F7.
                    // We pass every chunk; higher layer reassembles.
                    let _ = cb_tx.send(BusEvent::Received(msg.to_vec()));
                };
                // open input
                let mi = match MidiInput::new("katana-rs") {
                    Ok(m) => m,
                    Err(e) => { let _ = ev_tx.send(BusEvent::Disconnected(format!("open input: {e}"))); continue; }
                };
                let port = match mi.ports().iter().find(|p| mi.port_name(p).ok() == Some(in_name.clone())) {
                    Some(p) => p.clone(),
                    None => { let _ = ev_tx.send(BusEvent::Disconnected("input port not found".into())); continue; }
                };
                let conn = mi.connect(&port, "katana-rs", input_cb, ());
                match conn {
                    Ok(c) => {
                        _input = Some(c);
                        // open output
                        let mo = match MidiOutput::new("katana-rs") { Ok(m) => m, Err(e) => { let _ = ev_tx.send(BusEvent::Disconnected(format!("open output: {e}"))); continue; } };
                        let oport = match mo.ports().iter().find(|p| mo.port_name(p).ok() == Some(out_name.clone())) {
                            Some(p) => p.clone(),
                            None => { let _ = ev_tx.send(BusEvent::Disconnected("output port not found".into())); continue; }
                        };
                        match mo.connect(&oport, "katana-rs") {
                            Ok(o) => {
                                output = Some(o);
                                *status.lock() = Status { connected: true, in_port: in_name, out_port: out_name };
                                let _ = ev_tx.send(BusEvent::Connected);
                            }
                            Err(e) => { let _ = ev_tx.send(BusEvent::Disconnected(format!("open output conn: {e}"))); }
                        }
                    }
                    Err(e) => { let _ = ev_tx.send(BusEvent::Disconnected(format!("connect input: {e}"))); }
                }
            }
            BusMsg::Send(data) => {
                if let Some(out) = &mut output {
                    if out.send(&data).is_err() {
                        let _ = ev_tx.send(BusEvent::Disconnected("output send failed".into()));
                    }
                }
            }
        }
    }
}
