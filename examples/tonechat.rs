//! Throwaway e2e probe: chat via Ollama with agent context, apply returned
//! sparse changes to a patch, push to amp, then read back from amp.
//! Run: cargo run --example tonechat
use katana_rs_proto::agent::{Agent, AgentConfig, Provider, system_prompt, apply_sparse_changes, parse_lenient_json};
use katana_rs_proto::patch::Patch;
use katana_rs_proto::amp::AmpApi;
use katana_rs_proto::schema::Schema;
use katana_rs_proto::midi::MidiBus;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Schema::load(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/midi.xml")))?;
    let tpl = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default.syx"));
    let mut patch = Patch::from_template(tpl, "KATANA Mk2".into());

    let cfg = AgentConfig {
        provider: Provider::Compatible,
        base_url: "http://localhost:11434/v1".into(),
        model: "qwen2.5:7b".into(),
        api_key: None,
        timeout_secs: 300,
    };
    let agent = Agent::new(cfg);
    let context = system_prompt(&schema, &patch);
    println!("context len: {} chars", context.len());

    let user = "Give me a Mesa Boogie Dual Rectifier high gain tone. Name the patch and give sparse changes.";
    let messages: Vec<(String, String)> = vec![
        ("system".into(), context.clone()),
        ("user".into(), user.into()),
    ];
    let reply = agent.chat(&messages)?;
    println!("--- FULL model reply ---\n{}", reply);

    if let Some(v) = parse_lenient_json(&reply) {
        if let Some(changes) = v.get("changes") {
            if changes.is_array() {
                let n = apply_sparse_changes(&mut patch, changes)?;
                println!("applied {n} sparse changes");
                // show key bytes
                println!("amp type byte [0,0x21] = {:02X}", patch.byte(0, 0x21).unwrap_or(0));
                println!("gain byte [0,0x12] = {:02X}", patch.byte(0, 0x12).unwrap_or(0));
            } else {
                println!("changes not array: {}", changes);
            }
        } else {
            println!("no changes key: {}", v);
        }
    } else {
        println!("parse failed");
    }

    let bus = MidiBus::spawn();
    bus.set_device("KATANA:KATANA MIDI 1 20:0".into(), "KATANA:KATANA MIDI 1 20:0".into());
    std::thread::sleep(std::time::Duration::from_millis(500));
    let api = AmpApi::new(&bus);
    api.push_patch(&patch);
    println!("pushed patch to amp temp buffer");
    Ok(())
}
