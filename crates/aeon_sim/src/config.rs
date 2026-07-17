//! Campaign configuration.

use aeon_core::calendar::GameDate;

/// Everything that defines a fresh campaign before any player decision.
///
/// Together with the ordered player-command log, this fully determines the
/// campaign: same config plus same commands must always produce the same
/// simulation state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CampaignConfig {
    /// Player-facing campaign name; changeable later via a command.
    pub name: String,
    /// The campaign seed every derived random stream folds in.
    pub seed: u64,
    /// The campaign's first day. The first tick advances to the day after.
    pub start_date: GameDate,
}
