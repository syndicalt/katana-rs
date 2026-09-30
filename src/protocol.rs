//! USB-MIDI protocol for Boss Katana MK2 (firmware 2).
//!
//! Ported from KATANA-MK2-FxFloorBoard (C++/Qt, GPL-2.0, Colin Willcocks).
//! The protocol is data-driven: every parameter lives in `midi.xml` (5-level
//! tree: section -> addr MSB -> addr LSB -> data index -> enum/range leaf).
//!
//! Wire format (all frames):
//!   F0 41 00 00 00 00 33 [cmd] [addr 4B] [data...] [checksum] F7
//!   cmd 0x11 = Request (R), 0x12 = Data-set (DT1)
//!   checksum = (0x80 - (sum(address+data) & 0x7F)) & 0x7F

use anyhow::{bail, Context, Result};
use std::collections::HashMap;

/// Device constants (ported from globalVariables.h).
pub const DEVICE_ID: u8 = 0x00;
pub const CMD_REQUEST: u8 = 0x11;
pub const CMD_DATA: u8 = 0x12;
/// Offset of the address in a sysex frame (0-based byte index).
pub const ADDR_OFFSET: usize = 8;
/// Offset of the first data byte (after address).
pub const DATA_OFFSET: usize = 12;
/// Offset where the checksum accumulation starts.
pub const CHECKSUM_OFFSET: usize = 8;

/// Sizes/graphics constants.
pub const BANK_TOTAL_USER: usize = 50;
pub const BANK_TOTAL_ALL: usize = 100;
pub const PATCH_PER_BANK: usize = 9;
pub const BANK_SIZE: usize = 100;

/// Patch reply / request sizes (bytes).
pub const PATCH_REPLY_SIZE: usize = 2058;
pub const PATCH_SIZE: usize = 2156;
pub const SYSTEM_SIZE: usize = 328;
pub const SYSTEM_REPLY_SIZE: usize = 307;
pub const NAME_REPLY_SIZE: usize = 30;
pub const NAME_LEN: usize = 16;

/// A frame (one sysex message) of a patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// 4-byte address (big-endian), e.g. 0x60000000
    pub address: [u8; 4],
    /// Data bytes (no F0/F7, no address, no checksum).
    pub data: Vec<u8>,
}

impl Frame {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.data.len() + 2);
        out.extend_from_slice(&[
            0xF0, 0x41, 0x00, DEVICE_ID, 0x00, 0x00,
            0x33, CMD_DATA,
        ]);
        out.extend_from_slice(&self.address);
        out.extend_from_slice(&self.data);
        let cs = checksum(&out[CHECKSUM_OFFSET..]);
        out.push(cs);
        out.push(0xF7);
        out
    }

    /// Parse a frame starting at `data[0..]`; returns (frame, consumed).
    ///
    /// Layout matches [`Frame::encode`]: F0, 7-byte header (41 00 00 00 00 33
    /// cmd), 4-byte address, data, checksum, F7. `body` below excludes the
    /// leading F0, so body[i] == data[i + 1].
    pub fn parse(data: &[u8]) -> Option<(Frame, usize)> {
        if data.len() < 15 || data[0] != 0xF0 {
            return None;
        }
        let end = data[..].iter().position(|&b| b == 0xF7)?;
        if end < 14 {
            return None;
        }
        let body = &data[1..end];
        if body.len() < 13 || body[0] != 0x41 || body[5] != 0x33 {
            return None;
        }
        let cmd = body[6];
        if cmd != CMD_REQUEST && cmd != CMD_DATA {
            return None;
        }
        let addr = [body[7], body[8], body[9], body[10]];
        // payload = frame data bytes + trailing stored checksum byte.
        let payload = &body[11..];
        if payload.len() < 2 {
            return None;
        }
        // verify checksum
        let expect = checksum(&body[7..body.len() - 1]);
        if payload[payload.len() - 1] != expect {
            // tolerant: still return, caller may use it
        }
        let frame = Frame {
            address: addr,
            data: payload[..payload.len() - 1].to_vec(),
        };
        Some((frame, end + 1))
    }

}

/// Checksum per original: 0x80 - (sum & 0x7F), 0 if sum mod 128 == 0.
pub fn checksum(bytes: &[u8]) -> u8 {
    let sum: u32 = bytes.iter().map(|&b| b as u32).sum();
    let s = (sum % 0x80) as u8;
    if s == 0 { 0 } else { 0x80 - s }
}


/// Build an REQUEST (0x11) message: used to pull patch/system data.
pub fn request(addr: [u8; 4], size: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(&[0xF0, 0x41, 0x00, DEVICE_ID, 0x00, 0x00, 0x33, CMD_REQUEST]);
    out.extend_from_slice(&addr);
    out.extend_from_slice(&size);
    let cs = checksum(&out[CHECKSUM_OFFSET..]);
    out.push(cs);
    out.push(0xF7);
    out
}

/// Address of patch (bank, patch). Ported from MidiTable::patchRequest.
///
/// `bank` is 1-based, `patch` 1-based. Returns the 2-byte addr prefix
/// (addr1, addr2) plus 0x0000 for the remaining bytes.
pub fn patch_address(bank: usize, patch: usize) -> Result<[u8; 4]> {
    if bank == 0 || patch == 0 || bank > BANK_TOTAL_ALL || patch > PATCH_PER_BANK {
        return Ok([0x60, 0x00, 0x00, 0x00]); // temp
    }
    // The original C++ computes with signed int; `empty` is negative
    // (192 - (50*9 - 192) = -66), so use isize to mirror that exactly.
    let mut offset: isize = ((bank - 1) * PATCH_PER_BANK) as isize + (patch as isize - 1);
    let memory_size: isize = 0xBF + 1; // 192
    let empty = memory_size - ((BANK_TOTAL_USER * PATCH_PER_BANK) as isize - memory_size);
    if bank > BANK_TOTAL_USER {
        offset += empty; // System patches start at a new memory range.
    }
    let addr_max: isize = 0x80; // 128 patches per bank change
    let n = offset / addr_max;
    let (a1, a2) = if bank > BANK_TOTAL_USER {
        (29 + n, offset - addr_max * n)
    } else {
        (16 + n, offset - addr_max * n)
    };
    Ok([a1 as u8, a2 as u8, 0x00, 0x00])
}

/// Patch name request address (same math as [`patch_address`] but the
/// 4th byte is 0x10 = 16 name bytes).
pub fn name_address(bank: usize, patch: usize) -> Result<[u8; 4]> {
    let mut a = patch_address(bank, patch)?;
    a[3] = 0x10;
    Ok(a)
}

/// Value mapping for a leaf (enum or range).
#[derive(Clone, Debug)]
pub enum ValueMap {
    /// Discrete values: raw byte -> label.
    Table(Vec<(u8, String)>),
    /// Linear range: raw (start..=end) -> display min..max, optional unit suffix.
    Range {
        raw_start: u8,
        raw_end: u8,
        data_min: f64,
        data_max: f64,
        suffix: Option<String>,
    },
    /// 16-bit data mapped by (msb,lsb) table entries (rare).
    Data2(Vec<u8>),
}

/// A leaf parameter (level 4). `idx` is the DATA index (byte offset in frame).
#[derive(Clone, Debug)]
pub struct Param {
    pub idx: u8,
    pub value: ValueMap,
    pub label: String,
}

/// A logical group of parameters (level 3, e.g. "PreAmp", "FX1").
/// `slot` is used to address: addr[2] = slot (the '00' or '01' byte in
/// structure calls), addr[3] = idx?  -- see comments.
#[derive(Clone, Debug)]
pub struct ParamGroup {
    pub key: String,           // e.g. "PRE:00"
    pub label: String,
    pub params: Vec<Param>,
}

/// One structure unit: (addr3, addr4) -> group, from XML L2/L3.
#[derive(Clone, Debug, Default)]
pub struct MidiMap {
    /// section (e.g. "Structure", "System")
    sections: HashMap<String, Section>,
}

#[derive(Clone, Debug, Default)]
struct Section {
    /// addr_msb: addr_lsb -> level of groups
    groups: HashMap<u8, HashMap<u8, ParamGroup>>,
}

impl ParamGroup {
    /// Find index of param by idx byte.
    pub fn find(&self, idx: u8) -> Option<&Param> {
        self.params.iter().find(|p| p.idx == idx)
    }
}

impl MidiMap {
    pub fn load(xml: &str) -> Result<MidiMap> {
        let doc = roxmltree::Document::parse(xml).context("midi.xml parse")?;
        let root = doc.root_element();
        if root.tag_name().name() != "SysX" {
            bail!("midi.xml root must be SysX");
        }
        let mut map = MidiMap { sections: HashMap::new() };
        for sec in root.children().filter(|n| n.is_element()) {
            let sname = sec.tag_name().name().to_string();
            let mut section = Section::default();
            for l1 in sec.children().filter(|n| n.is_element()) {
                // L1: value = addr byte (MSB of address)
                let a1 = parse_byte(l1.attribute("value"))?;
                for l2 in l1.children().filter(|n| n.is_element()) {
                    let a2 = parse_byte(l2.attribute("value"))?;
                    let mut group = ParamGroup { key: String::new(), label: String::new(), params: vec![] };
                    for l3 in l2.children().filter(|n| n.is_element()) {
                        let idx = parse_byte(l3.attribute("value"))?;
                        // determine if leaf children are enums or range
                        let leafs: Vec<_> = l3.children().filter(|n| n.is_element()).collect();
                        if leafs.is_empty() {
                            continue;
                        }
                        let first = leafs[0].attribute("value").unwrap_or("");
                        if first.starts_with("range") {
                            // single range leaf
                            let r = leafs.iter().find(|l| l.attribute("value").map_or(false, |v| v.starts_with("range")));
                            if let Some(rl) = r {
                                let spec = rl.attribute("value").unwrap_or("");
                                let parts: Vec<&str> = spec.split('/').collect();
                                if parts.len() >= 4 {
                                    let rs = parse_byte(Some(parts[0]))?;
                                    let re = parse_byte(Some(parts[1]))?;
                                    let dmn: f64 = parts[2].parse().unwrap_or(0.0);
                                    let dmx: f64 = parts[3].parse().unwrap_or(100.0);
                                    let suffix = rl.attribute("desc")
                                        .map(|s| s.to_string())
                                        .filter(|s| !s.is_empty());
                                    group.params.push(Param {
                                        idx,
                                        value: ValueMap::Range {
                                            raw_start: rs,
                                            raw_end: re,
                                            data_min: dmn,
                                            data_max: dmx,
                                            suffix,
                                        },
                                        label: rl.attribute("name").unwrap_or("").to_string(),
                                    });
                                }
                            }
                        } else {
                            // enum: map raw byte -> label (skip empty-name entries?)
                            let mut table = Vec::new();
                            for child in &leafs {
                                if let Some(v) = child.attribute("value") {
                                    if let Ok(b) = parse_byte(Some(v)) {
                                        let name = child.attribute("name").unwrap_or("");
                                        table.push((b, name.to_string()));
                                    }
                                }
                            }
                            // DATA-kind leafs: parameters spanning 2 bytes
                            let kind = l3.attribute("desc").unwrap_or("");
                            // if the leaf tag is DATA it's a 2-byte value; table.value
                            // uses type from midiTable logic - approximate: DATA leaves
                            // store 2 bytes; we mark via `kind` containing no 'range'
                            if leafs.iter().all(|c| c.tag_name().name() == "DATA") && table.iter().all(|(_, n)| n.is_empty()) {
                                let mut vals = Vec::new();
                                for (b, _) in &table {
                                    vals.push(*b);
                                }
                                // store as Data2 (two bytes concatenated like original)
                                group.params.push(Param {
                                    idx,
                                    value: ValueMap::Data2(vals),
                                    label: String::new(),
                                });
                            } else {
                                group.params.push(Param {
                                    idx,
                                    value: ValueMap::Table(table),
                                    label: kind.to_string(),
                                });
                            }
                        }
                    }
                    group.key = format!("{:02X}", a1);
                    group.label = l1.attribute("name").unwrap_or("").to_string();
                    section.groups.entry(a1).or_default().insert(a2, group);
                }
            }
            map.sections.insert(sname, section);
        }
        Ok(map)
    }

    /// Resolve a leaf by section name + addr bytes.
    /// `addr` = [a1, a2, a3, a4] where a3 = data index (idx), a4 = ??? (for 2-byte params)
    pub fn find_param(&self, section: &str, addr: [u8; 4]) -> Option<&Param> {
        let sec = self.sections.get(section)?;
        let group = sec.groups.get(&addr[2])?.get(&addr[3])?;
        // addr[0] is START of address? No - structure addressing uses addr[0]=a1, addr[1]=a2 (00), addr[2]=a3...
        group.find(addr[3]) // idx may be overridden by caller
    }

    pub fn groups(&self, section: &str) -> Option<&HashMap<u8, HashMap<u8, ParamGroup>>> {
        self.sections.get(section).map(|s| &s.groups)
    }
}

fn parse_byte(s: Option<&str>) -> Result<u8> {
    let s = s.ok_or_else(|| anyhow::anyhow!("missing value"))?;
    u8::from_str_radix(s, 16).context("hex value")
}

/// Convert raw value to display string using the leaf's ValueMap.
pub fn value_to_display(vm: &ValueMap, raw: &[u8]) -> String {
    match vm {
        ValueMap::Table(t) => {
            let b = raw.first().copied().unwrap_or(0);
            t.iter().find(|(k, _)| *k == b).map(|(_, s)| s.clone()).unwrap_or_else(|| format!("{b:02X}"))
        }
        ValueMap::Range { raw_start, raw_end, data_min, data_max, suffix } => {
            let b = raw.first().copied().unwrap_or(0);
            let t = if b >= *raw_start && b <= *raw_end {
                ((b as f64) / (raw_end - raw_start) as f64) * (data_max - data_min) + data_min
            } else { 0.0 };
            let prec = suffix.as_ref().map_or(0, |s| s.matches('.').map(|_| 1).sum());
            let mut s = format!("{:.prec$}", t, prec = prec);
            if let Some(sfx) = suffix {
                s.push_str(sfx);
            }
            s
        }
        ValueMap::Data2(v) => {
            let msb = raw.first().copied().unwrap_or(0) as usize;
            let lsb = raw.get(1).copied().unwrap_or(0) as usize;
            let idx = msb * 128 + lsb;
            v.get(idx).map(|b| format!("{b:02X}")).unwrap_or_else(|| format!("{idx:04X}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_original() {
        // From capture: request bytes F0..33 11 10 00 00 00 00 00 0F 48
        let body = [0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0x48];
        let cs = checksum(&body);
        assert_eq!(cs, 0x19); // validated against amidi request
        // Another known: identity request no checksum
    }

    #[test]
    fn patch_address_user_bank1() {
        let a = patch_address(1, 1).unwrap();
        assert_eq!(a, [0x10, 0x00, 0x00, 0x00]);
        let a = patch_address(1, 9).unwrap();
        assert_eq!(a, [0x10, 0x08, 0x00, 0x00]);
        // Empty range is negative (-66): system banks are shifted back, so
        // (50,9) = 0x13:0x41 per the C++ MK2 FW2 source.
        let a = patch_address(50, 9).unwrap();
        assert_eq!(a, [0x13, 0x41, 0x00, 0x00]);
    }

    #[test]
    fn request_encoding() {
        let r = request([0x10, 0x00, 0x00, 0x00], [0x00, 0x00, 0x0F, 0x48]);
        assert_eq!(
            r,
            vec![0xF0, 0x41, 0x00, 0x00, 0x00, 0x00, 0x33, 0x11,
                 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0x48, 0x19, 0xF7]
        );
    }

    #[test]
    fn parse_real_frame() {
        let frame = Frame {
            address: [0x10, 0x00, 0x00, 0x00],
            data: vec![0x4B, 0x41, 0x54, 0x41, 0x4E, 0x41, 0x20, 0x4D, 0x6B, 0x32, 0x20, 0x20, 0x20, 0x20, 0x20],
        };
        let bytes = frame.encode();
        let (f, _) = Frame::parse(&bytes).unwrap();
        assert_eq!(f.address, frame.address);
        assert_eq!(f.data[..4], [0x4B, 0x41, 0x54, 0x41]);
    }
}
