//! Verify live-follow against the REAL amp: enable verbose, request patch,
//! then watch for any knob-change frames the user turns on the amp.
//! Run: cargo run --example livetest
use katana_rs_proto::amp::AmpApi;
use katana_rs_proto::live;
use katana_rs_proto::midi::{BusEvent, MidiBus};
use katana_rs_proto::patch::Patch;
use katana_rs_proto::protocol;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bus = MidiBus::spawn();
    bus.set_device("KATANA:KATANA MIDI 1 20:0".into(), "KATANA:KATANA MIDI 1 20:0".into());
    std::thread::sleep(std::time::Duration::from_millis(500));
    let api = AmpApi::new(&bus);
    api.enable_verbose();
    let _ = api.request_patch(1, 1)?;
    println!("listening for live changes (turn a knob on the amp!) ...");

    let mut frames = Vec::new();
    let mut applied_live = 0usize;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
    while std::time::Instant::now() < deadline {
        if let Some(BusEvent::Received(m)) = bus.try_recv() {
            frames.extend_from_slice(&m);
            // parse complete frames
            let mut i = 0;
            while i < frames.len() {
                if frames[i] == 0xF0 {
                    if let Some(pos) = frames[i..].iter().position(|&b| b == 0xF7) {
                        let end = i + pos + 1;
                        let fb = frames[i..end].to_vec();
                        frames.drain(..end);
                        if let Some((f, _)) = protocol::Frame::parse(&fb) {
                            if live::is_live_frame(&f) && f.data.len() <= 32 {
                                applied_live += 1;
                                println!(
                                    "  LIVE: sec {} idx {} -> {:02X?}",
                                    f.address[2], f.address[3], f.data
                                );
                            } else {
                                println!("  patch frame: len={}", fb.len());
                            }
                        }
                        continue;
                    }
                }
                i += 1;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!("done: {applied_live} live changes observed");
    let tpl = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default.syx"));
    let _p = Patch::from_template(tpl, "x".into());
    Ok(())
}
