//! LLM chat agent: OpenAI-compatible provider client, persisted settings,
//! tone-research skill prompt, patch context serialization, and application
//! of model-returned parameter changes onto a [`Patch`].
//!
//! The agent speaks the OpenAI `chat/completions` wire protocol so both
//! api.openai.com and local OpenAI-compatible servers (Ollama, LM Studio)
//! are usable; only the base URL / key / model differ.

use crate::patch::Patch;
use crate::protocol::Frame;
use crate::schema::Schema;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Prompt paragraph describing the primary tone-replication function and
/// where to research tone descriptions / existing patches.
const TONE_SKILL: &str = "\
Tone research skill:
Your primary function is to help the user replicate a tone. When asked about a \
band, song, or music style, research (from your own knowledge, and by naming \
the closest bundled reference patch below) how to configure this Boss Katana \
MKII to sound like it, then answer with a JSON object of concrete parameter \
changes. Good places to look for tone descriptions and existing matches:
- The bundled reference patches below (real Katana MKII patches exported from \
Boss Tone Studio; each contains a full hex parameter set; you can name one as \
a starting point, e.g. \"start from `ACDC Tone.tsl`\").
- Patches traded in online Katana/FxFloorBoard communities (they use the same \
patch format),
- Live/studio recordings, tab/music-gear articles and forum threads describing \
the artist's amp and pedal chain;
- Boss Tone Studio's own patch library and official Katana MKII presets.
When you name a reference patch, say which of its settings you would keep and \
which to change, so the user understands the reasoning.\n";

/// The response JSON we instruct the model to produce. Two accepted shapes:
/// sparse changes (preferred — one byte per change) or blob (`UserPatch%`
/// hex arrays as in `.tsl` files). [`apply_tsl_json`] accepts both.
const OUTPUT_FORMAT: &str = "\
Return ONLY a JSON object — no markdown fences, no prose outside JSON.
For \"changes\" use the sparse form (preferred, exact and short):
{\"patch_name\": \"short name\",
 \"changes\": [
   {\"section\": 0, \"index\": 33, \"value\": 80},
   {\"section\": 0, \"index\": 51, \"value\": \"0F\"}
 ],
 \"explanation\": \"one or two sentences: what you changed and why\"}
- \"section\" is the section number in the context heading, e.g. \"## PRE (section 00)\" -> 0.
- \"index\" is the [idx 0x..] number shown next to each current value in the context.
- \"value\" is decimal 0-255 or a two-digit hex string.
Alternative blob form (like the bundled .tsl files) is also accepted:
{\"changes\": {\"UserPatch%Patch_0\": [\"01\",...], \"UserPatch%PatchName\": [\"31\",...]}}
Only include parameters you are actually changing; the patch keeps every other
current value. Name the patch in \"patch_name\".\n";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// LLM provider. Both variants are OpenAI-compatible; they differ only in
/// base URL / key conventions and defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "compatible")]
    Compatible,
}

/// Persisted agent settings (`~/.config/katana-rs/config.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub provider: Provider,
    /// Base URL of the OpenAI-compatible API root (includes `/v1`).
    pub base_url: String,
    pub model: String,
    /// Cap on a single chat request; local models with big context can be
    /// slow, so this defaults high (5 min).
    pub api_key: Option<String>,
    pub timeout_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig::for_provider(Provider::Compatible)
    }
}

impl AgentConfig {
    pub fn for_provider(provider: Provider) -> Self {
        match provider {
            Provider::OpenAI => AgentConfig {
                provider,
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4o-mini".into(),
                api_key: std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty()),
                timeout_secs: 120,
            },
            Provider::Compatible => AgentConfig {
                provider,
                base_url: "http://localhost:11434/v1".into(),
                model: "qwen2.5:7b".into(),
                api_key: None,
                timeout_secs: 300,
            },
        }
    }

    /// Path of the JSON config file.
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("katana-rs").join("config.json"))
    }

    /// Load config from disk; returns defaults if the file is missing or
    /// unreadable, and reports parse errors.
    pub fn load() -> AgentConfig {
        let Some(path) = Self::config_path() else {
            return AgentConfig::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => AgentConfig::default(),
        }
    }

    /// Save config to disk, creating parent directories.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path().context("no config dir")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Effective API key: explicit setting, else `OPENAI_API_KEY` env.
    pub fn effective_api_key(&self) -> Option<String> {
        if let Some(k) = &self.api_key {
            if !k.is_empty() {
                return Some(k.clone());
            }
        }
        std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty())
    }

    /// URL for the chat completions endpoint.
    pub fn chat_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{base}/chat/completions")
        }
    }
}

// ---------------------------------------------------------------------------
// Agent client
// ---------------------------------------------------------------------------

/// Stateless chat client for one conversation; threads can own one freely.
#[derive(Clone, Debug)]
pub struct Agent {
    pub config: AgentConfig,
}

/// A chat message: (role, content).
pub type ChatMessage = (String, String);

impl Agent {
    pub fn new(config: AgentConfig) -> Self {
        Agent { config }
    }

    /// Send a chat request. `messages` already includes the system prompt.
    pub fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let key = self.config.effective_api_key();
        if key.is_none() && self.config.provider == Provider::OpenAI {
            log::warn!("agent: OpenAI provider without API key (set OPENAI_API_KEY or in settings)");
        }
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
            .collect();
        let body = serde_json::json!({
            "model": self.config.model,
            "messages": msgs,
            "temperature": 0.4,
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs.max(1)))
            .build()?;
        let mut req = client.post(self.config.chat_url()).json(&body);
        if let Some(k) = key {
            req = req.bearer_auth(k);
        }
        let resp = req
            .send()
            .with_context(|| format!("POST {}", self.config.chat_url()))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("LLM HTTP {status}: {}", text.chars().take(300).collect::<String>());
        }
        let v: serde_json::Value = resp
            .json()
            .context("parse LLM response (not JSON)?")?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("LLM response missing content"))
    }
}

// ---------------------------------------------------------------------------
// Context + skill
// ---------------------------------------------------------------------------

/// Build a compact serialization of the CURRENT patch so the model can see
/// every current value. Uses schema labels and raw patch bytes.
pub fn build_agent_context(schema: &Schema, patch: &Patch) -> String {
    let name = if patch.name.is_empty() {
        String::from_utf8_lossy(&patch.name_bytes()).trim_end().to_string()
    } else {
        patch.name.clone()
    };
    let mut out = String::new();
    out.push_str(&format!("Current patch: name={name:?}\n"));
    // These sections hold the audible chain; ASSIGN blocks (07..0E) are
    // footswitch/expression routing and are skipped to keep the prompt short.
    let mut sections: Vec<u8> = [0x00, 0x01, 0x03, 0x05, 0x06].to_vec();
    sections.retain(|a1| schema.group(*a1).is_some());
    for a1 in sections {
        let Some(g) = schema.group(a1) else { continue };
        out.push_str(&format!("## {} (section {a1:02X})\n", g.name));
        for p in &g.params {
            if p.label.is_empty() {
                continue;
            }
            if !matches!(p.value, crate::schema::ValueSpec::Enum(_)) {
                // Range/DATA values print their raw byte too; label only.
            }
            if let Some(raw) = patch.byte(a1 as usize, p.idx as usize) {
                out.push_str(&format!(
                    "  {} [idx {:#04X}] = {}\n",
                    p.label,
                    p.idx,
                    schema.display(a1, p.idx, raw)
                ));
            }
        }
    }
    out
}

/// Assembled system prompt: role, tone-research skill, reference patch
/// catalog, output format, and the current patch state.
pub fn system_prompt(schema: &Schema, patch: &Patch) -> String {
    let mut s = String::new();
    s.push_str(
        "You are a Boss Katana MKII tone-architect embedded in a patch editor. \
The user asks about a band/song/music style; you research it, decide how to \
configure the amp to replicate the tone, and return JSON that the editor \
applies to the current patch (then the user can load it and save it). You have \
full context of the amp's CURRENT configuration below.\n\n",
    );
    s.push_str(TONE_SKILL);
    s.push_str("\nReference patches bundled with this editor (assets/patches/, \
filename = tone description; each is a JSON .tsl with a `name` and a full \
`paramSet` in the same hex format you must return):\n");
    match patch_catalog() {
        Ok(list) => {
            for f in list {
                s.push_str(&format!("- {f}\n"));
            }
        }
        Err(e) => s.push_str(&format!("(catalog unavailable: {e})\n")),
    }
    s.push_str("\nCurrent patch state (all values are the live values; \
parameters not listed must stay unchanged):\n");
    s.push_str(&build_agent_context(schema, patch));
    s.push('\n');
    s.push_str(OUTPUT_FORMAT);
    s
}

/// List bundled `.tsl` reference patch files (sorted by file name).
fn patch_catalog() -> Result<Vec<String>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/patches");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("read {}", dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".tsl") {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------------------------
// apply_tsl_json
// ---------------------------------------------------------------------------

/// Byte offsets into the default.syx template for each `.tsl` parameter set,
/// ported from `sysxWriter::convertFromTSL` / `KATmk2_default_replace`
/// (KATANA-MK2-FxFloorBoard, GPL-2.0). `split` marks parameters stored in two
/// chunks: first `split` bytes at `offset`, the rest at `offset2`.
const TSL_OFFSETS: &[(&str, usize, Option<(usize, usize)>)] = &[
    ("PatchName", 12, None),
    ("Patch_0", 28, None),
    ("Fx(1)", 154, Some((296, 128))),
    ("Fx(2)", 438, Some((580, 128))),
    ("Delay(1)", 722, None),
    ("Delay(2)", 754, None),
    ("Patch_1", 786, Some((864, 64))),
    ("Patch_2", 896, None),
    ("Status", 944, None),
    ("KnobAsgn", 1006, None),
    ("ExpPedalAsgn", 1088, None),
    ("ExpPedalAsgnMinMax", 1136, None),
    ("GafcExp1Asgn", 1230, None),
    ("GafcExp1AsgnMinMax", 1278, None),
    ("GafcExp2Asgn", 1372, None),
    ("GafcExp2AsgnMinMax", 1420, None),
    ("Eq(2)", 108, None),
    ("FsAsgn", 2090, None),
    ("Patch_Mk2V2", 2098, None),
    ("Contour(1)", 2128, None),
    ("Contour(2)", 2136, None),
    ("Contour(3)", 2144, None),
];

/// Map a template byte offset to (frame index, data index) by walking the
/// patch's frame layout (header F0..+8 header+4 addr = 12, data, checksum+F7).
fn offset_to_cell(frames: &[Frame], offset: usize) -> Option<(usize, usize)> {
    let mut pos = 0usize;
    for (i, f) in frames.iter().enumerate() {
        let data_start = pos + 12;
        if offset >= data_start && offset < data_start + f.data.len() {
            return Some((i, offset - data_start));
        }
        pos += 14 + f.data.len();
    }
    None
}

/// Parse one byte of a JSON value: two-digit hex string (with or without
/// `0x`/`#`) or a 0..=255 number.
fn parse_byte(v: &serde_json::Value) -> Option<u8> {
    match v {
        serde_json::Value::String(s) => {
            let s = s.trim();
            let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
            u8::from_str_radix(s, 16).ok().or_else(|| s.parse::<u8>().ok())
        }
        serde_json::Value::Number(n) => n.as_u64().and_then(|n| u8::try_from(n).ok()),
        _ => None,
    }
}

/// Write a byte array at a template offset; silently skips out-of-range bytes.
fn write_at(patch: &mut Patch, offset: usize, bytes: &[u8]) -> usize {
    let mut written = 0usize;
    for (j, b) in bytes.iter().enumerate() {
        if let Some((sec, idx)) = offset_to_cell(&patch.frames, offset + j) {
            patch.set_byte(sec, idx, *b);
            written += 1;
        }
    }
    written
}

/// Apply an agent response (or a raw `.tsl` `paramSet`) onto `patch`.
///
/// Accepts either the full response `{ "changes": {...}, ... }`, a raw TSL
/// document `{ "data": [[{ "paramSet": {...} }]], ... }`, or a bare
/// `UserPatch%... -> [hex bytes]` object. Returns the number of parameter
/// sets applied. The patch is marked dirty by the caller after applying.
pub fn apply_tsl_json(patch: &mut Patch, json_value: &serde_json::Value) -> Result<usize> {
    let map = if let Some(ch) = json_value.get("changes") {
        if ch.is_object() {
            ch
        } else {
            bail!("`changes` must be a JSON object")
        }
    } else if let Some(ps) = json_value.get("paramSet") {
        if ps.is_object() { ps } else { bail!("`paramSet` must be an object") }
    } else if let Some(data) = json_value.get("data").and_then(|d| d.get(0)).and_then(|d| d.get(0)).and_then(|d| d.get("paramSet")) {
        data
    } else if json_value.is_object() {
        json_value
    } else {
        bail!("agent response is not a JSON object")
    };

    let mut applied = 0usize;
    for (name, _value) in map.as_object().expect("object checked above") {
        let Some(value) = map.get(name) else { continue };
        if !value.is_array() {
            continue;
        }
        let short = name.strip_prefix("UserPatch%").unwrap_or(name);
        let Some(&(_, offset, second)) = TSL_OFFSETS.iter().find(|(n, _, _)| *n == short) else {
            continue; // e.g. `patch_name` / `explanation` or unknown key
        };
        let bytes: Vec<u8> = value
            .as_array()
            .expect("checked above")
            .iter()
            .filter_map(parse_byte)
            .collect();
        if bytes.is_empty() {
            continue;
        }
        write_at(patch, offset, &bytes);
        if let Some((offset2, split)) = second {
            if bytes.len() > split {
                write_at(patch, offset2, &bytes[split..]);
            }
        }
        applied += 1;
    }
    Ok(applied)
}

/// Apply sparse changes: `changes` is an array of `{section, index, value}`
/// objects. Writes each byte onto the patch and returns how many were set.
pub fn apply_sparse_changes(patch: &mut Patch, changes: &serde_json::Value) -> Result<usize> {
    let arr = changes.as_array().ok_or_else(|| anyhow::anyhow!("sparse `changes` must be an array"))?;
    let mut applied = 0usize;
    for item in arr {
        let Some(obj) = item.as_object() else { continue };
        let section = obj.get("section").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let index = match obj.get("index").and_then(|v| v.as_u64()) {
            Some(i) => i as usize,
            None => {
                obj.get("idx").and_then(|v| {
                    v.as_str().and_then(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok()).map(|b| b as usize)
                }).unwrap_or(0)
            }
        };
        let value = obj.get("value").and_then(parse_byte);
        let Some(value) = value else { continue };
        patch.set_byte(section, index, value);
        applied += 1;
    }
    Ok(applied)
}

/// Tolerant JSON parse: strips markdown fences and trailing commas, and
/// drops `// comment` lines before parsing.
pub fn parse_lenient_json(text: &str) -> Option<serde_json::Value> {
    let mut s = text.trim().to_string();
    if s.starts_with("```") {
        let first_nl = s.find('\n').unwrap_or(s.len());
        s = s[first_nl..].trim_start_matches('\n').to_string();
    }
    if s.trim_end().ends_with("```") {
        let t = s.trim_end();
        s = t[..t.len() - 3].to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut escape = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for c2 in chars.by_ref() {
                if c2 == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    let re = regex_lite::Regex::new(r",(\s*[}\]])").ok()?;
    let s3 = re.replace_all(&out, "$1").to_string();
    serde_json::from_str(&s3).ok()
}

#[cfg(test)]
mod sparse_tests {
    use super::*;

    #[test]
    fn sparse_applies_bytes() {
        let tpl = include_bytes!("../assets/default.syx");
        let mut patch = Patch::from_template(tpl, "X".into());
        let changes = serde_json::json!([
            {"section": 0, "index": 33, "value": 80},
            {"section": 0, "index": 34, "value": "0F"}
        ]);
        let n = apply_sparse_changes(&mut patch, &changes).unwrap();
        assert_eq!(n, 2);
        assert_eq!(patch.byte(0, 33), Some(80));
        assert_eq!(patch.byte(0, 34), Some(0x0F));
    }

    #[test]
    fn lenient_parse_with_comments() {
        let text = "```json\n{\"a\": 1, // comment\n\"b\": [1,2,],}\n```";
        let v = parse_lenient_json(text).expect("must parse");
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"][1], 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template_patch(name: &str) -> Patch {
        let tpl = include_bytes!("../assets/default.syx");
        Patch::from_template(tpl, name.into())
    }

    #[test]
    fn provider_url_building() {
        let local = AgentConfig::for_provider(Provider::Compatible);
        assert_eq!(local.chat_url(), "http://localhost:11434/v1/chat/completions");
        let openai = AgentConfig::for_provider(Provider::OpenAI);
        assert_eq!(openai.chat_url(), "https://api.openai.com/v1/chat/completions");
        // Trailing slash / full-endpoint variants are normalized.
        let mut custom = local.clone();
        custom.base_url = "http://127.0.0.1:8080/v1/".into();
        assert_eq!(custom.chat_url(), "http://127.0.0.1:8080/v1/chat/completions");
        custom.base_url = "http://127.0.0.1:8080/v1/chat/completions".into();
        assert_eq!(custom.chat_url(), "http://127.0.0.1:8080/v1/chat/completions");
    }

    #[test]
    fn config_path_includes_dir() {
        let p = AgentConfig::config_path().unwrap();
        assert!(p.ends_with("katana-rs/config.json"));
    }

    #[test]
    fn agent_context_contains_current_values() {
        let schema = Schema::load(include_str!("../assets/midi.xml")).unwrap();
        let mut patch = template_patch("TESTP");
        // Amp type raw 0x17 => "[BROWN]" per midi.xml enum.
        patch.set_byte(0, 0x21, 0x17);
        let ctx = build_agent_context(&schema, &patch);
        assert!(ctx.contains("TESTP"), "context must include patch name");
        assert!(ctx.contains("[BROWN]"), "context must include amp type label");
        assert!(ctx.contains("PREAMP:"), "context must include preamp params");
    }

    #[test]
    fn apply_tsl_json_real_patch() {
        let tsl: serde_json::Value =
            serde_json::from_str(include_str!("../assets/patches/12 string.tsl")).unwrap();
        let mut patch = template_patch("NEW");
        let n = apply_tsl_json(&mut patch, &tsl).unwrap();
        assert_eq!(n, 17, "all UserPatch% keys of the tsl must be applied");

        // PatchName 16 bytes -> frame0 data idx 0.. = "12 string ".
        let nb = patch.name_bytes();
        assert_eq!(&nb[..10], b"12 string ");
        // Patch_0 @28 => frame0 data idx 16.. : Amp Type byte at idx 0x21.
        assert_eq!(patch.byte(0, 0x21), Some(0x01), "amp type byte (Patch_0+0x11)");
        // Fx(1) first 128 bytes @154 => frame1 data.
        assert_eq!(patch.byte(1, 0), Some(0x01));
        assert_eq!(patch.byte(1, 1), Some(0x1F));
        // Fx(1) remainder @296 => frame2 data.
        assert_eq!(patch.byte(2, 0), Some(0x32));
        // Delay(1) @722 => frame5 data.
        assert_eq!(patch.byte(5, 0), Some(0x01));
        // Patch_1 (fw1: 50 bytes, all in first chunk) @786 => frame5 idx 64.
        assert_eq!(patch.byte(5, 64), Some(0x01));
        // Status @944 => frame6 data idx 80.
        assert_eq!(patch.byte(6, 80), Some(0x00));
    }

    #[test]
    fn apply_tsl_json_agent_response() {
        let mut patch = template_patch("NEW");
        let resp = serde_json::json!({
            "patch_name": "Clean",
            "changes": {
                "UserPatch%PatchName": ["43","4C","45","41","4E","20","20","20","20","20","20","20","20","20","20","20"],
                "UserPatch%Patch_0": ["01","01","46","3C","43","00","32","22","29","00","32","32","32","32","32","00","01","08","53","0A","3A","3E","46","3C","19","00","01","00","32","00","01","01","05","64","00","00","32","32","05","05","32","32","32","07","0A","0A","02","00","01","00","00","10","0E","01","15","17","01","14","1B","0E","1D","00","00","00","00","00","00","00","00","00","00","00"]
            },
            "explanation": "clean base"
        });
        let n = apply_tsl_json(&mut patch, &resp).unwrap();
        assert_eq!(n, 2);
        assert_eq!(patch.name_bytes()[..5], *b"CLEAN");
        // Amp type byte in Patch_0 chunk = "08" => [CLEAN].
        assert_eq!(patch.byte(0, 0x21), Some(0x08));
        // Unknown keys are ignored.
        let n2 = apply_tsl_json(&mut patch, &serde_json::json!({
            "changes": {"UserPatch%Nope": ["00"], "patch_name": "x"},
            "explanation": "y"
        })).unwrap();
        assert_eq!(n2, 0);
        // Bare map (no wrapper) is accepted.
        let n3 = apply_tsl_json(&mut patch, &serde_json::json!({
            "Delay(1)": ["01","00","00","54"]
        })).unwrap();
        assert_eq!(n3, 1);
        assert_eq!(patch.byte(5, 0), Some(0x01));
    }
}
