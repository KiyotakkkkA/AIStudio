use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let source = manifest.join("../../packages/shared/contract.channels.json");
    println!("cargo:rerun-if-changed={}", source.display());

    let text = fs::read_to_string(&source)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", source.display()));
    let channels: Vec<String> = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not a JSON string array: {error}", source.display()));

    let mut generated = String::from("pub const CHANNELS: &[&str] = &[\n");
    for channel in &channels {
        writeln!(generated, "    {channel:?},").expect("write to String");
    }
    generated.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("channels.rs");
    fs::write(&out, generated)
        .unwrap_or_else(|error| panic!("cannot write {}: {error}", out.display()));
}
