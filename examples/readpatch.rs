//! Throwaway probe: request real patch (1,1), accumulate 2058-byte reply,
//! merge via Patch::from_reply, print the patch name read back.
//! Run: cargo run --example readpatch
use midir::{MidiInput, MidiOutput};
use std::sync::mpsc::channel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mi = MidiInput::new("probe")?;
    let mo = MidiOutput::new("probe")?;
    let ip = mi.ports().into_iter().find(|p| mi.port_name(p).map(|n| n.to_uppercase().contains("KATANA")).unwrap_or(false)).ok_or("no in")?;
    let op = mo.ports().into_iter().find(|p| mo.port_name(p).map(|n| n.to_uppercase().contains("KATANA")).unwrap_or(false)).ok_or("no out")?;
    let mut out = mo.connect(&op, "probe")?;
    let (tx, rx) = channel::<Vec<u8>>();
    let _in = mi.connect(&ip, "probe", move |_ts, msg, _| { let _ = tx.send(msg.to_vec()); }, ())?;
    // request patch bank 1 patch 1 (addr 0x10000000 size 0x0F48, checksum 0x19)
    out.send(&[0xF0,0x41,0x00,0x00,0x00,0x00,0x33,0x11,0x10,0x00,0x00,0x00,0x00,0x00,0x0F,0x48,0x19,0xF7])?;
    let mut buf: Vec<u8> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while buf.len() < katana_rs_proto::protocol::PATCH_REPLY_SIZE && std::time::Instant::now() < deadline {
        if let Ok(msg) = rx.recv_timeout(std::time::Duration::from_millis(300)) {
            buf.extend_from_slice(&msg);
        }
    }
    let tpl = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default.syx"))?;
    let patch = katana_rs_proto::patch::Patch::from_reply(&tpl, &buf);
    let name = String::from_utf8_lossy(&patch.name_bytes()).trim_end().to_string();
    println!("patch name from amp: '{name}'");
    let tpl_patch = katana_rs_proto::patch::Patch::from_template(&tpl, String::new());
    println!("template name: '{}'", String::from_utf8_lossy(&tpl_patch.name_bytes()).trim_end());
    Ok(())
}
