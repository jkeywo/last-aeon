//! Headless authoritative simulation for The Last Aeons.
//!
//! The simulation runs on Bevy ECS with no renderer, window, or asset
//! plugins. Native and web clients attach presentation to this same
//! simulation; nothing outside this crate owns or alters gameplay rules.

use bevy::app::{App, Plugin};

/// Root plugin installing the authoritative simulation into a Bevy [`App`].
///
/// Clients and the headless host both install exactly this plugin, which is
/// what keeps native, web, and test simulations identical.
pub struct AeonSimPlugin;

impl Plugin for AeonSimPlugin {
    fn build(&self, _app: &mut App) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_plugin_installs_headlessly() {
        let mut app = App::new();
        app.add_plugins(AeonSimPlugin);
        app.update();
    }
}
