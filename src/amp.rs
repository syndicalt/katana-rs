//! Amp API: high-level patch transfer over the existing [`MidiBus`].
//!
//! All wire-format helpers are pure functions (unit-testable); the
//! [`AmpApi`] wrapper only submits assembled sysex to the bus.

use crate::midi::{BusMsg, MidiBus};
use crate::patch::Patch;
use crate::protocol::{self, Frame};
use anyhow::Result;
use std::sync::mpsc::Sender;

/// Size (bytes) requestable with one patch request; matches
/// `globalVariables.h: patchRequestDataSize = "00000F48"`.
pub const PATCH_REQUEST_SIZE: [u8; 4] = [0x00, 0x00, 0x0F, 0x48];

/// Build the sysex that writes a patch to the temp buffer.
///
/// Port of `SysxIO::writeToBuffer`: every frame is re-addressed to
/// `0x60 0x00 <a3> <a4>` (temp buffer) and re-encoded with its checksum.
pub fn encode_patch_to_temp(patch: &Patch) -> Vec<u8> {
    let mut out = Vec::new();
    for f in &patch.frames {
        let addr = [0x60, 0x00, f.address[2], f.address[3]];
        let frame = Frame {
            address: addr,
            data: f.data.clone(),
        };
        out.extend_from_slice(&frame.encode());
    }
    out
}

/// Build a patch request sysex (bank/patch 1-based; 0 = temp buffer).
/// Port of `MidiTable::patchRequest` (see `examples/readpatch.rs`).
pub fn request_message(bank: usize, patch: usize) -> Result<Vec<u8>> {
    let addr = protocol::patch_address(bank, patch)?;
    Ok(protocol::request(addr, PATCH_REQUEST_SIZE))
}

/// Build the "write current patch into memory slot" sysex.
///
/// Port of `patchWriteDialog::writeToMemory` (exact literal, including the
/// trailing `00` in the checksum position):
/// `F0 41 00 00 00 00 33 12 7F 00 01 04 00 {slot-1:02X} 00 F7`
pub fn write_slot_message(slot: usize) -> Vec<u8> {
    let addr = (slot.saturating_sub(1) & 0x7F) as u8;
    vec![
        0xF0, 0x41, 0x00, 0x00, 0x00, 0x00, 0x33, 0x12,
        0x7F, 0x00, 0x01, 0x04, 0x00, addr, 0x00, 0xF7,
    ]
}

/// Build the "enable verbose editor mode" sysex. In this mode the amp
/// streams live parameter-change frames (DT1) whenever a physical knob is
/// moved, so the editor UI can follow the amp. Port of
/// `SysxIO::setEditorMode` cmd==0: `F0 41 00 00 00 00 33 12 7F 00 00 01 01 7F F7`.
pub fn enable_verbose_message() -> Vec<u8> {
    vec![
        0xF0, 0x41, 0x00, 0x00, 0x00, 0x00, 0x33, 0x12,
        0x7F, 0x00, 0x00, 0x01, 0x01, 0x7F, 0xF7,
    ]
}



/// High-level operations on the Katana over an existing MIDI bus.
///
/// Holds only a cloned message sender; the bus IO thread is shared with the
/// rest of the app (device lifecycle stays in [`MidiBus`]).
#[derive(Clone)]
pub struct AmpApi {
    tx: Sender<BusMsg>,
}

impl AmpApi {
    pub fn new(bus: &MidiBus) -> Self {
        AmpApi { tx: bus.sender() }
    }

    /// Enable verbose editor mode so the amp streams live knob changes.
    pub fn enable_verbose(&self) {
        let _ = self.tx.send(BusMsg::Send(enable_verbose_message()));
    }

    /// Send the whole patch to the amp's temporary buffer (0x60xxxxxx),
    /// marking it dirty so downstream "save" flows know a write is pending.
    pub fn push_patch(&self, patch: &Patch) {
        self.tx
            .send(BusMsg::Send(encode_patch_to_temp(patch)))
            .ok();
    }

    /// Request a patch from memory (bank/patch 1-based; 0 = temp buffer).
    pub fn request_patch(&self, bank: usize, patch: usize) -> Result<()> {
        let msg = request_message(bank, patch)?;
        let _ = self.tx.send(BusMsg::Send(msg));
        Ok(())
    }

    /// Write the current patch (already in the temp buffer) into a memory
    /// slot. `slot` is 1-based (1..=9 per bank).
    pub fn write_slot(&self, slot: usize) {
        let _ = self.tx.send(BusMsg::Send(write_slot_message(slot)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_slot_exact_literal() {
        // Literal from patchWriteDialog.cpp slot 1 ("00").
        assert_eq!(
            write_slot_message(1),
            vec![0xF0, 0x41, 0x00, 0x00, 0x00, 0x00, 0x33, 0x12,
                 0x7F, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 0xF7]
        );
        // Slot 5 => addr byte 0x04.
        assert_eq!(write_slot_message(5)[13], 0x04);
        // Slot 0 is clamped like slot 1 (address must never underflow).
        assert_eq!(write_slot_message(0)[13], 0x00);
    }

    #[test]
    fn request_message_matches_capture() {
        // Same bytes as examples/readpatch.rs / protocol test capture.
        let r = request_message(1, 1).unwrap();
        assert_eq!(
            r,
            vec![0xF0, 0x41, 0x00, 0x00, 0x00, 0x00, 0x33, 0x11,
                 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0x48, 0x19, 0xF7]
        );
    }

    #[test]
    fn encode_patch_rewrites_addresses_to_temp() {
        let tpl = include_bytes!("../assets/default.syx");
        let mut patch = Patch::from_template(tpl, "X".into());
        // Simulate a patch that was loaded from memory bank 1 (0x10...).
        for f in &mut patch.frames {
            f.address[0] = 0x10;
        }
        let out = encode_patch_to_temp(&patch);
        let frames = crate::patch::split_frames(&out);
        assert_eq!(frames.len(), 16);
        for f in &frames {
            assert_eq!(f.address[0], 0x60);
            assert_eq!(f.address[1], 0x00);
        }
        // Section byte (a3) preserved: frame 2 keeps 0x02.
        assert_eq!(frames[2].address[2], 0x02);
        // Temp-buffer frames reparse with identical data.
        assert_eq!(frames[0].data, patch.frames[0].data);
    }
}
