//! XML-driven patch parameter schema.
//!
//! `midi.xml` layout (5 levels under a section):
//!   SysX > Header|Address|System|Structure|Tables|Footer
//!   Structure > L1(value=addr byte A1) > L2(value=addr A2) > L3(value=idx)
//!     > L4(value=PARAM raw, name=display) or L4(value=range, name=str/end/min/max)
//!
//! Model faithfully mirrors the C++ MidiTable level tree so that lookups
//! used by the UI resolve exactly like the original app.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;

/// A discrete enum option: raw byte -> display text.
#[derive(Clone, Debug)]
pub struct EnumOpt {
    pub raw: u8,
    pub label: String,
    pub help: String,
}

/// A range mapping: raw byte interval -> display numeric range.
#[derive(Clone, Debug)]
pub struct RangeSpec {
    pub raw_start: u8,
    pub raw_end: u8,
    pub data_min: f64,
    pub data_max: f64,
    pub suffix: String,
}

/// A value map attached to one parameter slot (L4).
#[derive(Clone, Debug)]
pub enum ValueSpec {
    Enum(Vec<EnumOpt>),
    Range(RangeSpec),
    /// 2-byte data spanning multiple sub-slots (DATA type).
    Data(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct ParamSpec {
    /// L3 packet index (this is the `hex3` used by UI).
    pub idx: u8,
    /// Label (desc from L3, e.g. "BOOSTER:", or name)
    pub label: String,
    /// value spec from L4 leaf
    pub value: ValueSpec,
}

#[derive(Clone, Debug, Default)]
pub struct GroupSpec {
    /// L2 addr byte (A2, usually 0x00)
    pub addr2: u8,
    /// L1 group name, e.g. "PRE", "FX1"
    pub name: String,
    /// params keyed by L3 idx
    pub params: Vec<ParamSpec>,
}

impl GroupSpec {
    /// Find a param by its L3 index within this group.
    pub fn find(&self, idx: u8) -> Option<&ParamSpec> {
        self.params.iter().find(|p| p.idx == idx)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Schema {
    pub structure: HashMap<u8, GroupSpec>,
}
impl Schema {
    pub fn load(xml: &str) -> Result<Schema> {
        let doc = roxmltree::Document::parse(xml).context("parse midi.xml")?;
        let root = doc.root_element();
        if root.tag_name().name() != "SysX" {
            bail!("root must be <SysX>");
        }
        let mut schema = Schema::default();
        for sec in root.children().filter(|n| n.is_element()) {
            if sec.tag_name().name() != "Structure" {
                continue;
            }
            for l1 in sec.children().filter(|n| n.is_element()) {
                let a1 = hex_byte(l1.attribute("value"), "L1")?;
                let mut group = GroupSpec { addr2: 0, name: String::new(), params: vec![] };
                for l2 in l1.children().filter(|n| n.is_element()) {
                    let a2 = hex_byte(l2.attribute("value"), "L2")?;
                    group.addr2 = a2;
                    for l3 in l2.children().filter(|n| n.is_element()) {
                        let idx = hex_byte(l3.attribute("value"), "L3")?;
                        let param = build_param(l3, idx)?;
                        group.params.push(param);
                    }
                }
                group.name = l1.attribute("name").unwrap_or("").to_string();
                schema.structure.insert(a1, group);
            }
        }
        Ok(schema)
    }

    pub fn group(&self, addr1: u8) -> Option<&GroupSpec> {
        self.structure.get(&addr1)
    }

    /// Find a param by its L3 index within a group.
    pub fn find(&self, addr1: u8, idx: u8) -> Option<&ParamSpec> {
        self.group(addr1)?.params.iter().find(|p| p.idx == idx)
    }

    /// Enumerate display options for an enum param.
    pub fn enum_labels(&self, addr1: u8, idx: u8) -> Option<&[EnumOpt]> {
        match &self.find(addr1, idx)?.value {
            ValueSpec::Enum(v) => Some(v),
            _ => None,
        }
    }

    /// Raw byte -> display string for leaf.
    pub fn display(&self, addr1: u8, idx: u8, raw: u8) -> String {
        let Some(p) = self.find(addr1, idx) else { return format!("{raw:02X}") };
        match &p.value {
            ValueSpec::Enum(v) => value_to_enum(v, raw),
            ValueSpec::Range(r) => value_to_range(r, raw),
            ValueSpec::Data(_) => format!("{raw:02X}"),
        }
    }
}

fn build_param(l3: roxmltree::Node, idx: u8) -> Result<ParamSpec> {
    let label = l3.attribute("desc").unwrap_or("").to_string();
    let leafs: Vec<_> = l3.children().filter(|n| n.is_element()).collect();
    if leafs.is_empty() {
        return Ok(ParamSpec { idx, label, value: ValueSpec::Enum(vec![]) });
    }
    // Each L4 is a value entry.
    let mut enums = Vec::new();
    let mut range_spec: Option<RangeSpec> = None;
    for leaf in &leafs {
        let v = leaf.attribute("value").unwrap_or("");
        if v == "range" {
            let spec = leaf.attribute("name").unwrap_or("");
            let p: Vec<&str> = spec.split('/').collect();
            if p.len() >= 4 {
                range_spec = Some(RangeSpec {
                    raw_start: u8::from_str_radix(p[0], 16).unwrap_or(0),
                    raw_end: u8::from_str_radix(p[1], 16).unwrap_or(0x7F),
                    data_min: p[2].parse().unwrap_or(0.0),
                    data_max: p[3].parse().unwrap_or(100.0),
                    suffix: leaf.attribute("desc").unwrap_or("").to_string(),
                });
            }
        } else {
            let raw = hex_byte(Some(v), "L4")?;
            let name = leaf.attribute("name").unwrap_or("");
            let help = leaf.attribute("desc").unwrap_or("");
            enums.push(EnumOpt { raw, label: name.to_string(), help: help.to_string() });
        }
    }
    let value = if let Some(r) = range_spec {
        ValueSpec::Range(r)
    } else if enums.iter().any(|e| !e.label.is_empty()) {
        ValueSpec::Enum(enums)
    } else {
        // DATA-type: values without names; 2-byte lookup handled by UI code
        ValueSpec::Data(enums.iter().map(|e| e.raw).collect())
    };
    Ok(ParamSpec { idx, label, value })
}

/// Map raw byte to enum display; if all empty names, show hex.
pub fn value_to_enum(v: &[EnumOpt], raw: u8) -> String {
    if let Some(e) = v.iter().find(|e| e.raw == raw) {
        if !e.label.is_empty() {
            return e.label.clone();
        }
    }
    format!("{raw:02X}")
}

/// Linear range conversion (ports MidiTable::rangeToValue for Range).
pub fn value_to_range(r: &RangeSpec, raw: u8) -> String {
    let span = (r.raw_end - r.raw_start) as f64;
    let data_span = r.data_max - r.data_min;
    let t = if r.raw_start == r.raw_end { 0.0 } else {
        (raw as f64 - r.raw_start as f64) / span * data_span + r.data_min
    };
    // original uses precision from data_max string decimal length (or 0)
    let prec = if r.suffix.contains('.') { 1 } else { 0 };
    let s = format!("{t:.prec$}");
    if r.suffix.is_empty() { s } else { format!("{s}{}", r.suffix) }
}

fn hex_byte(v: Option<&str>, what: &str) -> Result<u8> {
    let s = v.ok_or_else(|| anyhow::anyhow!("missing {what}"))?;
    u8::from_str_radix(s.trim(), 16).with_context(|| format!("{what}: invalid hex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_real_xml_and_find() {
        let xml = include_str!("../assets/midi.xml");
        let schema = Schema::load(xml).unwrap();
        // PRE group exists
        let g = schema.group(0x00).expect("PRE group");
        assert_eq!(g.name, "PRE");
        // BOOSTER gain at idx 0x13 is a range -50/+50 over 00/64
        let p = g.find(0x13).expect("gain");
        match &p.value {
            ValueSpec::Range(r) => {
                assert_eq!(r.raw_start, 0x00);
                assert_eq!(r.raw_end, 0x64);
                assert_eq!(r.data_min, -50.0);
                assert_eq!(r.data_max, 50.0);
            }
            _ => panic!("expected range"),
        }
        // display at middle = 0
        let s = schema.display(0x00, 0x13, 0x32);
        assert_eq!(s, "0");
        // enum: PRE gain type idx 0x21 has 30 amps
        let e = schema.enum_labels(0x00, 0x21).expect("enum");
        assert!(e.len() > 5);
    }

    #[test]
    fn hex_byte_parses() {
        assert_eq!(hex_byte(Some("00"), "x").unwrap(), 0);
        assert_eq!(hex_byte(Some("0F"), "x").unwrap(), 15);
    }
}
