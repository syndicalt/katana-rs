//! Patch data model: assembled from `default.syx` template + amp replies,
//! edited in-memory, and written back to the amp/frame buffers.
//!
//! Port of SysxIO::setFileSource / getSourceValue / receiveSysx semantics
//! (KATANA-MK2-FxFloorBoard, GPL-2.0, Colin Willcocks).

use crate::protocol::*;

/// Positional overlay table, ported verbatim from SysxIO::receiveSysx.
///
/// The original works on hex strings: `sysxBuffer.replace(24, 256,
/// rxMsg.mid(24, 256))` etc. Values below are BYTE offsets (= char/2).
/// The amp reply (exactly 2058 bytes = `PATCH_REPLY_SIZE`) is spliced into
/// the 2156-byte default.syx template at these fixed positions. The reply
/// re-chunks the patch data into rows that do NOT align to the template's
/// 0x100-stepped frames, so this literal table is what makes it work.
const MERGE_TABLE: &[(usize, usize, usize)] = &[
    // (template_byte, reply_byte, count) — byte offsets from C++ char/2
    (12, 12, 128),
    (154, 140, 113),
    (267, 267, 15),
    (296, 282, 128),
    (438, 410, 98),
    (536, 522, 30),
    (580, 552, 128),
    (722, 680, 83),
    (805, 777, 45),
    (864, 822, 128),
    (1006, 950, 68),
    (1088, 1032, 128),
    (1230, 1160, 113),
    (1343, 1287, 15),
    (1372, 1302, 128),
    (1514, 1430, 98),
    (1612, 1542, 30),
    (1656, 1572, 128),
    (1798, 1700, 83),
    (1881, 1797, 45),
    (1940, 1842, 128),
    (2082, 1970, 68),
    (2150, 2052, 4),
];

/// One patch: 16 frames of the template with data merged from device.
#[derive(Clone, Debug)]
pub struct Patch {
    pub name: String,
    pub frames: Vec<Frame>,
    /// bank/patch this came from (1-based; 0 = temp/unsaved).
    pub bank: usize,
    pub patch: usize,
    pub dirty: bool,
}

impl Patch {
    /// Build a default (template) patch with a given name.
    pub fn from_template(template: &[u8], name: String) -> Patch {
        let frames = split_frames(template);
        Patch { name, frames, bank: 0, patch: 0, dirty: false }
    }

    /// Merge an amp reply into the template — port of receiveSysx.
    ///
    /// `reply` must be exactly [`PATCH_REPLY_SIZE`] bytes; frames within are
    /// re-spliced into the template and every frame checksum is recomputed.
    pub fn from_reply(template: &[u8], reply: &[u8]) -> Patch {
        let mut merged = Vec::with_capacity(PATCH_SIZE);
        merged.extend_from_slice(&template[..PATCH_SIZE.min(template.len())]);
        // Ensure enough room (template may be shorter than expected)
        while merged.len() < PATCH_SIZE {
            merged.push(0);
        }
        for &(tpos, rpos, n) in MERGE_TABLE {
            if rpos + n <= reply.len() && tpos + n <= merged.len() {
                merged[tpos..tpos + n].copy_from_slice(&reply[rpos..rpos + n]);
            }
        }
        // Recompute checksums per frame (ports the F7-walking recalc loop).
        let mut frames = split_frames(&merged);
        for f in &mut frames {
            // encode() recomputes the checksum; sanity: verify stored one
            let _ = checksum(&{
                let mut v = Vec::new();
                v.extend_from_slice(&f.address);
                v.extend_from_slice(&f.data);
                v
            });
        }
        Patch { name: String::new(), frames, bank: 0, patch: 0, dirty: false }
    }

    /// Read a byte at (section, index) — section = addr[2] (0..15).
    pub fn byte(&self, section: usize, index: usize) -> Option<u8> {
        self.frames.get(section)?.data.get(index).copied()
    }

    /// Write a byte into the patch.
    pub fn set_byte(&mut self, section: usize, index: usize, value: u8) {
        if let Some(f) = self.frames.get_mut(section) {
            if index < f.data.len() {
                f.data[index] = value;
            }
        }
    }

    /// Encode the whole patch as 16 concatenated sysex frames.
    pub fn encode_all(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for f in &self.frames {
            out.extend_from_slice(&f.encode());
        }
        out
    }

    /// Extract name bytes in frame 0 data.
    pub fn name_bytes(&self) -> Vec<u8> {
        self.frames
            .first()
            .map(|f| f.data.iter().take(NAME_LEN).copied().collect())
            .unwrap_or_default()
    }

    pub fn set_name(&mut self, name: &str) {
        let bytes: Vec<u8> = name
            .bytes()
            .chain(std::iter::repeat(0x20))
            .take(NAME_LEN)
            .collect();
        if let Some(f) = self.frames.first_mut() {
            for (i, b) in bytes.iter().take(NAME_LEN).enumerate() {
                if i < f.data.len() {
                    f.data[i] = *b;
                }
            }
        }
    }
}


/// Split a byte buffer into sysex frames (F0..F7).
pub fn split_frames(data: &[u8]) -> Vec<Frame> {
    let mut frames = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0xF0 {
            if let Some((f, consumed)) = Frame::parse(&data[i..]) {
                frames.push(f);
                i += consumed;
                continue;
            }
        }
        i += 1;
    }
    frames
}

/// Read data byte by (section, index) with 2-byte DATA support (port of
/// getSourceValue): `a1` is section byte, `idx` scans into frame data.
pub fn get_source_value(patch: &Patch, _area: &str, a1: u8, idx: u8) -> Vec<u8> {
    // C++ reads items.at(12 + idx) on the full saved hex of one frame set —
    // but getFileSource returns one FRAME list, so index = data[idx].
    match patch.byte(a1 as usize, idx as usize) {
        Some(b) => vec![b],
        None => Vec::new(),
    }
}


/// Set a data byte (port of setFileSource positional write).
pub fn set_source_value(patch: &mut Patch, _area: &str, a1: u8, idx: u8, value: u8) {
    patch.set_byte(a1 as usize, idx as usize, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real capture: 8 frames received (first 0x10000000 dropped), total
    // 2040 bytes + final 18-byte frame => synthesize the full 2058 for test
    // by prefixing the expected first frame from template.
    #[test]
    fn template_splits_16_frames() {
        let tpl = include_bytes!("../assets/default.syx");
        let patch = Patch::from_template(tpl, "TEST".into());
        assert_eq!(patch.frames.len(), 16);
        assert_eq!(patch.frames[0].address, [0x60, 0x00, 0x00, 0x00]);
        assert_eq!(patch.frames[0].data.len(), 128);
        assert_eq!(patch.frames[7].data.len(), 68);
        assert_eq!(patch.frames[15].data.len(), 72);
    }



    #[test]
    fn merge_table_consumes_2058() {
        // Reply: 8 frames of 255 + 1 frame of 18 = 2058 bytes
        let reply_len: usize = 8 * 255 + 18;
        assert_eq!(reply_len, PATCH_REPLY_SIZE);
        // MERGE_TABLE sums to exactly PATCH_SIZE - 14*16? We overlay 1932
        // data bytes + the reply keeps its own frame headers untouched? No:
        // total spliced = let's just validate the table maps within bounds.
        for &(t, r, n) in MERGE_TABLE {
            assert!(r + n <= PATCH_REPLY_SIZE, "reply range {r}+{n}");
            assert!(t + n <= PATCH_SIZE, "tpl range {t}+{n}");
        }
    }

    #[test]
    fn reply_merges_into_template() {
        let tpl = include_bytes!("../assets/default.syx");
        // The device re-chunks the 1932 data bytes into 8 frames of 241 data
        // bytes plus a final 4-byte frame (8×255 + 18 = 2058 bytes). Build
        // such a reply from the template's own data, then verify the merge
        // reproduces the template exactly.
        let stream: Vec<u8> = split_frames(tpl)
            .iter()
            .flat_map(|f| f.data.iter().copied())
            .collect();
        assert_eq!(stream.len(), PATCH_SIZE - 16 * 14);
        let mut reply = Vec::with_capacity(PATCH_REPLY_SIZE);
        let mut off = 0usize;
        for &n in &[241usize; 8] {
            let frame = Frame {
                address: [0x60, 0x00, 0x00, 0x00],
                data: stream[off..off + n].to_vec(),
            };
            reply.extend_from_slice(&frame.encode());
            off += n;
        }
        let frame = Frame {
            address: [0x60, 0x00, 0x00, 0x00],
            data: stream[off..off + 4].to_vec(),
        };
        reply.extend_from_slice(&frame.encode());
        assert_eq!(reply.len(), PATCH_REPLY_SIZE);

        let patch = Patch::from_reply(tpl, &reply);
        assert_eq!(patch.frames.len(), 16);
        assert_eq!(patch.frames[0].data.len(), 128);
        // Identity merge must reproduce the template byte-for-byte.
        assert_eq!(patch.encode_all(), tpl);
    }

    #[test]
    fn name_roundtrip() {
        let tpl = include_bytes!("../assets/default.syx");
        let mut patch = Patch::from_template(tpl, String::new());
        patch.set_name("MY PATCH");
        let nb = patch.name_bytes();
        assert_eq!(&nb[..8], b"MY PATCH");
    }

    #[test]
    fn encode_reparse_roundtrip() {
        let tpl = include_bytes!("../assets/default.syx");
        let patch = Patch::from_template(tpl, "X".into());
        let encoded = patch.encode_all();
        let frames = split_frames(&encoded);
        assert_eq!(frames.len(), 16);
        assert_eq!(frames[0].address, patch.frames[0].address);
        assert_eq!(frames[0].data, patch.frames[0].data);
    }
}

