//! Engine-agnostic foundations for The Last Aeons.
//!
//! This crate deliberately has no Bevy dependency: everything here must be
//! usable from the headless simulation, the tools CLI, and tests without
//! pulling in an engine. It owns the deterministic building blocks the whole
//! game rests on — stable game identity, seeded RNG streams, the campaign
//! calendar, fixed-point arithmetic, and canonical state hashing.

pub mod calendar;
pub mod fixed;
pub mod hash;
pub mod id;
pub mod rng;

/// The player-facing name of the game.
pub const GAME_NAME: &str = "The Last Aeons";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_name_is_stable() {
        assert_eq!(GAME_NAME, "The Last Aeons");
    }
}
