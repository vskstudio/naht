//! Pins the Luau plugin's outdated-token constant to the Rust protocol constant, so a bump on one side
//! that the other misses fails here instead of letting an outdated plugin read as current.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use naht_core::protocol;

type TestResult = Result<(), Box<dyn Error>>;

fn plugin_source(relative: &str) -> Result<String, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugin")
        .join(relative);
    Ok(fs::read_to_string(path)?)
}

#[test]
fn luau_outdated_plugin_token_equals_the_rust_constant() -> TestResult {
    let protocol_luau = plugin_source("src/Protocol.luau")?;
    let expected_line = format!(
        "Protocol.OUTDATED_PLUGIN_TOKEN = \"{}\"",
        protocol::OUTDATED_PLUGIN_TOKEN
    );

    assert!(
        protocol_luau
            .lines()
            .any(|line| line.trim() == expected_line),
        "plugin/src/Protocol.luau does not declare `{expected_line}`"
    );
    Ok(())
}
