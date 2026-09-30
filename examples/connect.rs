//! Throwaway connectivity probe: opens the Katana MIDI port, sends the
//! identity request, prints the reply. Run: cargo run --example connect
use midir::{MidiInput, MidiOutput};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mi = MidiInput::new("probe")?;
    let mo = MidiOutput::new("probe")?;
    let ins = mi.ports();
    let outs = mo.ports();
    println!("input ports: {:?}", ins.iter().map(|p| mi.port_name(p).unwrap_or_default()).collect::<Vec<_>>());
    println!("output ports: {:?}", outs.iter().map(|p| mo.port_name(p).unwrap_or_default()).collect::<Vec<_>>());
    let Some(op) = outs.iter().find(|p| mo.port_name(p).map(|n| n.to_uppercase().contains("KATANA")).unwrap_or(false)).cloned() else {
        println!("no KATANA output port found");
        return Ok(());
    };
    let Some(ip) = ins.iter().find(|p| mi.port_name(p).map(|n| n.to_uppercase().contains("KATANA")).unwrap_or(false)).cloned() else {
        println!("no KATANA input port found");
        return Ok(());
    };
    let mut out = mo.connect(&op, "probe")?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let _in = mi.connect(&ip, "probe", move |_ts, msg, _| { let _ = tx.send(msg.to_vec()); }, ())?;
    // identity request
    out.send(&[0xF0, 0x7E, 0x00, 0x06, 0x01, 0xF7])?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if let Ok(msg) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
            println!("reply: {:02X?}", msg);
            break;
        }
    }
    Ok(())
}
