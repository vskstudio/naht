//! Structured logging for the daemon and CLI (Stage 8).
//!
//! `naht-core` stays free of logging dependencies — it returns data, and the binary decides how to
//! surface it. Verbosity comes from `-v` flags, but an explicit `NAHT_LOG`/`RUST_LOG` env filter
//! always wins, so an operator can dial in per-target levels without touching flags.

use tracing_subscriber::EnvFilter;

/// Resolve the log directive: an env filter (`NAHT_LOG`, then `RUST_LOG`) overrides `-v`; otherwise
/// the repeat count picks the level (`info` → `debug` → `trace`, with `info` the default).
fn resolve_directive(verbose: u8, naht_log: Option<String>, rust_log: Option<String>) -> String {
    let from_env = naht_log
        .filter(|value| !value.is_empty())
        .or_else(|| rust_log.filter(|value| !value.is_empty()));
    if let Some(filter) = from_env {
        return filter;
    }
    match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    }
    .to_string()
}

/// Install the global tracing subscriber, writing to stderr. Idempotent in practice — called once
/// from `main`.
pub fn init(verbose: u8) {
    let directive = resolve_directive(
        verbose,
        std::env::var("NAHT_LOG").ok(),
        std::env::var("RUST_LOG").ok(),
    );
    // A bad env filter falls back to the verbosity level rather than aborting startup.
    let filter = EnvFilter::try_new(&directive)
        .unwrap_or_else(|_| EnvFilter::new(resolve_directive(verbose, None, None)));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbosity_count_maps_to_level() {
        assert_eq!(resolve_directive(0, None, None), "info");
        assert_eq!(resolve_directive(1, None, None), "debug");
        assert_eq!(resolve_directive(2, None, None), "trace");
        assert_eq!(resolve_directive(5, None, None), "trace");
    }

    #[test]
    fn env_filter_overrides_verbosity() {
        // NAHT_LOG wins over -v entirely.
        assert_eq!(
            resolve_directive(2, Some("naht=warn".to_string()), None),
            "naht=warn"
        );
        // RUST_LOG is the fallback env source.
        assert_eq!(
            resolve_directive(0, None, Some("debug".to_string())),
            "debug"
        );
        // NAHT_LOG beats RUST_LOG when both are set.
        assert_eq!(
            resolve_directive(0, Some("trace".to_string()), Some("warn".to_string())),
            "trace"
        );
    }

    #[test]
    fn empty_env_values_are_ignored() {
        assert_eq!(resolve_directive(1, Some(String::new()), None), "debug");
        assert_eq!(
            resolve_directive(0, Some(String::new()), Some(String::new())),
            "info"
        );
    }
}
