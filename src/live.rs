//! Live-follow: apply single parameter-change frames streamed by the amp.
//!
//! When the Katana MK2 is in "verbose editor mode" it pushes DT1 frames on
//! knob changes (e.g. `F0 41 00 00 00 00 33 12 60 00 00 22 3C cs F7`).
//! The address `60 00 <section> <idx>` targets the temp-patch region; the
//! frame data bytes are the new values. The app applies them to its in-memory
//! patch so the UI follows the amp — without sending anything back.

use crate::patch::Patch;
use crate::protocol::Frame;

/// True if the frame targets the temp-patch region (live parameter writes).
pub fn is_live_frame(f: &Frame) -> bool {
    // Frames written by the amp while the user turns knobs target the
    // editor temp region (60 00 ...).
    f.address[0] == 0x60 && f.address[1] == 0x00
}

/// Apply a live frame to the patch. The address layout matches the patch
/// configuration frames: `60 00 <section> <idx>`, where section = addr[2]
/// (the 0x00..0x0F block) and idx = addr[3] (byte index within the block).
/// Any number of data bytes are written sequentially starting at that index.
pub fn apply_live(patch: &mut Patch, f: &Frame) -> usize {
    if !is_live_frame(f) {
        return 0;
    }
    let section = f.address[2] as usize;
    let start = f.address[3] as usize;
    let mut written = 0;
    for (i, b) in f.data.iter().enumerate() {
        patch.set_byte(section, start + i, *b);
        written += 1;
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Patch;

    fn tpl() -> Patch {
        Patch::from_template(include_bytes!("../assets/default.syx"), String::new())
    }

    #[test]
    fn applies_single_byte_live_change() {
        let mut p = tpl();
        // gain (section 0, idx 0x22) -> 0x3C (60)
        let f = Frame { address: [0x60, 0x00, 0x00, 0x22], data: vec![0x3C] };
        assert!(is_live_frame(&f));
        assert_eq!(apply_live(&mut p, &f), 1);
        assert_eq!(p.byte(0, 0x22), Some(0x3C));
    }

    #[test]
    fn applies_batch_live_change() {
        let mut p = tpl();
        let f = Frame { address: [0x60, 0x00, 0x05, 0x00], data: vec![0x01, 0x02, 0x03] };
        assert_eq!(apply_live(&mut p, &f), 3);
        assert_eq!(p.byte(5, 0), Some(0x01));
        assert_eq!(p.byte(5, 1), Some(0x02));
        assert_eq!(p.byte(5, 2), Some(0x03));
    }

    #[test]
    fn ignores_system_frames() {
        let mut p = tpl();
        let f = Frame { address: [0x00, 0x01, 0x00, 0x00], data: vec![0x42] };
        assert!(!is_live_frame(&f));
        assert_eq!(apply_live(&mut p, &f), 0);
    }

    #[test]
    fn parses_encoded_live_frame() {
        let f = Frame { address: [0x60, 0x00, 0x00, 0x22], data: vec![0x3C] };
        let bytes = f.encode();
        let (parsed, _) = Frame::parse(&bytes).expect("encodes round-trip");
        assert_eq!(parsed.address, f.address);
        assert_eq!(parsed.data, f.data);
    }
}
