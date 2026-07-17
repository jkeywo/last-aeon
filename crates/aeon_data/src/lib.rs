//! Authored-content pipeline for The Last Aeons.
//!
//! Owns the Rhai script host and the definitions of authored content (jobs,
//! scenarios, results, events). Scripts receive validated read-only context
//! and emit typed effects; nothing in this crate mutates simulation state
//! directly.

/// Builds the restricted Rhai engine used for all authored content.
///
/// The full sandboxing surface (deterministic APIs only, no filesystem, no
/// wall-clock) is configured here so every consumer shares one policy.
pub fn content_engine() -> rhai::Engine {
    let mut engine = rhai::Engine::new();
    // Authored content must stay deterministic: no wall-clock access.
    engine.disable_symbol("timestamp");
    engine
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_evaluates_pure_expressions() {
        let engine = content_engine();
        let result: i64 = engine.eval("40 + 2").expect("pure expression evaluates");
        assert_eq!(result, 42);
    }
}
