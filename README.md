# katana-rs

Modern Rust editor for the Boss Katana MKII (Firmware 2) guitar amplifier.

A rewrite of [KATANA-MK2-FxFloorBoard](https://sourceforge.net/projects/fxfloorboard/) (C++/Qt, GPL-2.0, Colin Willcocks) with a modern `egui` UI and an **AI tone assistant** chat interface.

## Features

- **Amp connectivity** — talk to a real Katana MKII over USB-MIDI (ALSA via `midir`). Identity verification, full patch read, parameter changes, write to memory slot.
- **Protocol core** — SysEx builder/parser, checksum, bank/patch addressing, and the patch merge table ported faithfully from the original C++ source and validated against real hardware.
- **Modern UI** — dark themed `egui` interface with Amp / EQ / FX / Patch tabs.
- **AI tone assistant** *(the headline feature)* — chat with a configurable LLM agent that:
  - has **full context of the current amp configuration** (every parameter value, labeled via the `midi.xml` schema),
  - uses a **tone-research skill** (bundled reference-patch catalog: 84+ `.tsl` files, plus knowledge of amp models, effects, and where to find tone descriptions),
  - given a band/song/style, **returns concrete parameter changes as structured JSON**,
  - applies the changes to the current patch, **pushes them to the amp** (temp buffer), and lets you **save to a memory slot**,
  - supports **OpenAI API** and any **OpenAI-compatible local model** (Ollama, LM Studio, …) via provider/base URL/model/key settings.

## Usage

```bash
cargo run --release
```

1. The app opens with a chat panel and editor tabs.
2. On the **Connect** screen, pick the Katana MIDI ports (or auto-detect; device names contain "KATANA").
3. Chat: enter a tone request (e.g. *"make it sound like AC/DC's Back in Black"*). The agent returns a patch; click **Apply** to push it to the amp, then **Save to slot** to write it to a memory location.

### Built-in example probes

```bash
cargo run --example connect     # identity request → amp reply
cargo run --example readpatch   # request patch 1.1 → name + data round-trip
cargo run --example tonechat    # full LLM → changes → apply → amp (needs Ollama)
```

## Agent configuration

Settings persist in `~/.config/katana-rs/config.json` (provider, base URL, model, API key, timeout). The env var `OPENAI_API_KEY` is used as a fallback for the OpenAI provider.

- **Ollama (local, recommended for no API cost):** provider `Compatible`, base URL `http://localhost:11434/v1`, model e.g. `qwen2.5:7b`.
- **OpenAI:** provider `OpenAI`, key via settings or env. Model e.g. `gpt-4o-mini`.

## Architecture

```
src/
├── protocol.rs   # SysEx frame encode/parse, checksum, bank/patch addressing (port of MidiTable.cpp)
├── schema.rs     # midi.xml → typed parameter schema (label/value/range maps)
├── patch.rs      # patch model, default.syx template, amp-reply merge (receiveSysx port)
├── midi.rs       # midir ALSA transport thread + channel bus
├── amp.rs        # AmpApi: push patch to amp, request patch, write memory slot
├── agent.rs      # LLM client (OpenAI-compatible), config persistence, agent context, sparse/blob changes, tone skill
└── main.rs       # egui app: connect, editor tabs, chat panel
```

The protocol is **data-driven**: `assets/midi.xml` (7MB, from the current 2026 release binary) describes every parameter; `assets/default.syx` is the patch template. Both are consumed at compile time. `assets/patches/*.tsl` are the bundled reference tones.

## License

GPL-2.0 (inherited from the original KATANA-MK2-FxFloorBoard by Colin Willcocks; protocol ported from its source).
