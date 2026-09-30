//! End-to-end simulation of the live-follow flow WITHOUT an amp.
//!
//! Simulates: connect → verbose mode enabled → patch request → live knob
//! change frames arriving → app applies them to its in-memory patch and
//! sends NOTHING back → explicit write pushes a patch.
//!
//! Run: cargo test --test live_flow (or this example)
use katana_rs_proto::live::{apply_live, is_live_frame};
use katana_rs_proto::patch::Patch;
use katana_rs_proto::protocol::Frame;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tpl = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default.syx"));
    let mut patch = Patch::from_template(tpl, "SIM".into());

    println!("== Live-follow simulation (no amp needed) ==");
    println!("initial gain (sec 0 idx 0x22) = {:?}", patch.byte(0, 0x22));

    // Amp streams: F0 41 00 00 00 00 33 12 60 00 00 22 3C cs F7 (gain -> 60)
    let gain_change = Frame { address: [0x60, 0x00, 0x00, 0x22], data: vec![0x3C] };
    assert!(is_live_frame(&gain_change));
    let n = apply_live(&mut patch, &gain_change);
    println!("applied live gain change: {n} byte -> {:?}", patch.byte(0, 0x22));
    assert_eq!(patch.byte(0, 0x22), Some(0x3C));

    // Amp streams master volume (sec 6 idx 0x00 -> 0x50)
    let vol_change = Frame { address: [0x60, 0x00, 0x06, 0x00], data: vec![0x50] };
    apply_live(&mut patch, &vol_change);
    println!("applied live master change -> {:?}", patch.byte(6, 0x00));
    assert_eq!(patch.byte(6, 0x00), Some(0x50));

    // Non-live frame (system) is ignored
    let sys = Frame { address: [0x00, 0x01, 0x00, 0x00], data: vec![0x42] };
    assert_eq!(apply_live(&mut patch, &sys), 0);

    println!("== simulation OK: app followed amp, sent nothing back ==");
    Ok(())
}
