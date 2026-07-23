//! The interface: its design tokens, the data it reads, and the panels
//! built from them.

pub mod actions;
pub mod assignment_popup;
pub mod assignments_panel;
pub mod data;
pub mod dock;
pub mod forecast;
pub mod icons;
pub mod idle_panel;
pub mod inspector;
pub mod ledger_panel;
pub mod listing;
pub mod log_panel;
pub mod lookup;
pub mod overlays;
pub mod panel;
pub mod picker;
pub mod search;
pub mod shell;
#[cfg(not(target_arch = "wasm32"))]
pub mod specimen;
pub mod theme;
pub mod top_bar;
pub mod widgets;
