//! The ordered player-command pipeline.
//!
//! Every meaningful player decision enters the simulation as a
//! [`PlayerCommand`] wrapped in a [`CommandEnvelope`] carrying its execution
//! day and a monotonic sequence number. Commands validate at submission,
//! queue until their day's tick, and apply in strict `(day, seq)` order —
//! which is what makes a recorded command log replayable.

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use bevy::app::App;
use bevy::prelude::{IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::assignments::{
    self, ActiveAssignment, AssignmentRejection, AssignmentTarget, AssignmentsIndex,
};
use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::ids::{ArmyId, AssignmentId, CharacterId, ProvinceId, ShipId, WarId};
use crate::politics::PlayerHouse;
use crate::presence::{self, Location};
use crate::state::CampaignMeta;

/// A meaningful player decision.
///
/// Variants grow with each milestone; every variant must remain
/// deserialisable forever once a release has written it to a command log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PlayerCommand {
    /// Does nothing. Used by tests and as a log keep-alive.
    Noop,
    /// Renames the campaign.
    RenameCampaign {
        /// The new player-facing campaign name.
        name: String,
    },
    /// Starts a assignment for the player's organisation.
    StartAssignment {
        /// The assignment definition.
        assignment: ContentKey,
        /// The character who will lead it.
        leader: CharacterId,
        /// What the assignment acts on.
        target: AssignmentTarget,
    },
    /// Starts one of the assignment actions currently projected by a Situation.
    StartSituationAssignment {
        /// Exact structural Situation instance offering the action.
        situation: crate::situations::SituationInstanceKey,
        /// Authored action ID within the Situation definition.
        action: ContentKey,
        /// Character who will lead the ordinary authoritative assignment.
        leader: CharacterId,
        /// Concrete assignment target projected by the Situation.
        target: AssignmentTarget,
        /// Exact formal war authorising the action, when any.
        war: Option<WarId>,
    },
    /// Dismisses one persistent Situation resolution notice.
    DismissSituationResolution {
        /// Monotonic notice ID.
        resolution: u64,
    },
    /// Cancels one of the player's active assignments.
    CancelAssignment {
        /// The assignment to cancel.
        assignment: AssignmentId,
    },
    /// Answers a pending result popup.
    AnswerPopup {
        /// The popup being answered.
        popup: u64,
        /// The chosen option.
        choice: ContentKey,
    },
    /// Sends one of the player's characters travelling to a province.
    Travel {
        /// The traveller.
        character: CharacterId,
        /// The destination province.
        destination: ProvinceId,
    },
    /// Orders one of the player's ships to another province.
    MoveShip {
        /// The ship.
        ship: ShipId,
        /// The destination province.
        destination: ProvinceId,
    },
    /// Disbands one of the player's armies, returning its soldiers.
    DisbandArmy {
        /// The army.
        army: ArmyId,
    },
    /// Sets a standing order for one of the player's armies.
    SetStandingOrders {
        /// The army.
        army: ArmyId,
        /// What it may start on its own, in the order it reaches for them.
        orders: crate::warfare::StandingOrders,
    },
    /// Puts one of the player's ships under a named officer, or leaves it
    /// without one.
    SetShipCaptain {
        /// The ship.
        ship: ShipId,
        /// The officer taking command; `None` relinquishes it.
        captain: Option<CharacterId>,
    },
    /// Presses an advisory directive on a house that answers directly to
    /// the player. A wish, not an order: it lifts the pressure it names in
    /// the vassal head's own scoring, and the vassal remains free to do
    /// otherwise. Issuing a new one replaces the last.
    IssueDirective {
        /// The vassal the directive is pressed on.
        vassal: crate::ids::OrgId,
        /// The pressure the player wants the vassal to feel more keenly.
        intent: aeon_data::model::AiIntent,
        /// What it is aimed at, if anything.
        target: Option<AssignmentTarget>,
    },
    /// Withdraws the standing directive on one of the player's vassals.
    ClearDirective {
        /// The vassal whose directive is withdrawn.
        vassal: crate::ids::OrgId,
    },
    /// Sets a standing trade route on one of the player's transports.
    SetTradeRoute {
        /// The transport to route.
        ship: ShipId,
        /// The route it will ply.
        route: crate::trade::TradeRoute,
    },
    /// Clears the trade route on one of the player's ships.
    ClearTradeRoute {
        /// The ship whose route is cleared.
        ship: ShipId,
    },
}

/// A command bound to its execution day and order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    /// Global monotonic sequence number; total order within a day.
    pub seq: u64,
    /// The day this command executes, at the start of that day's tick.
    pub day: GameDate,
    /// The decision itself.
    pub command: PlayerCommand,
}

/// Why a submitted command was refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommandRejection {
    /// A campaign name was empty or unreasonably long.
    #[error("campaign names must be 1..=120 characters, got {length}")]
    InvalidCampaignName {
        /// Length of the rejected name in characters.
        length: usize,
    },
    /// A assignment-related command was refused.
    #[error(transparent)]
    Assignment(#[from] AssignmentRejection),
    /// A Situation action or notice was no longer available.
    #[error(transparent)]
    Situation(#[from] crate::situations::SituationError),
    /// No visible undismissed resolution has this ID.
    #[error("no such visible Situation resolution")]
    BadSituationResolution,
    /// A movement, disbanding, or posting change would invalidate active work.
    #[error("that force is committed to an active assignment")]
    ForceCommitted,
    /// A directive was aimed at a house that does not answer directly to
    /// the player.
    #[error("that house does not answer directly to you")]
    NotYourVassal,
}

/// Commands accepted but not yet applied, sorted by `(day, seq)`.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingCommands {
    entries: Vec<CommandEnvelope>,
}

impl PendingCommands {
    /// Inserts an envelope, keeping `(day, seq)` order.
    pub fn insert(&mut self, envelope: CommandEnvelope) {
        let key = (envelope.day, envelope.seq);
        let index = self.entries.partition_point(|e| (e.day, e.seq) <= key);
        self.entries.insert(index, envelope);
    }

    /// Removes and returns every envelope due on or before `date`, in order.
    pub fn take_due(&mut self, date: GameDate) -> Vec<CommandEnvelope> {
        let split = self.entries.partition_point(|e| e.day <= date);
        self.entries.drain(..split).collect()
    }

    /// The queued envelopes, in execution order.
    pub fn entries(&self) -> &[CommandEnvelope] {
        &self.entries
    }

    pub(crate) fn from_entries(entries: Vec<CommandEnvelope>) -> Self {
        let mut pending = Self::default();
        for envelope in entries {
            pending.insert(envelope);
        }
        pending
    }
}

/// The append-only record of accepted commands.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandLog {
    /// The next sequence number to assign.
    pub next_seq: u64,
    /// Every command applied so far, in application order.
    pub applied: Vec<CommandEnvelope>,
}

/// Whether an army is committed to an assignment that names it explicitly.
///
/// Commands are delayed, so callers use this both when accepting an order and
/// when applying it. That closes the gap where a force could become committed
/// after a destructive or movement command was queued.
fn army_has_active_assignment(world: &World, army: ArmyId) -> bool {
    world
        .get_resource::<AssignmentsIndex>()
        .is_some_and(|index| {
            index.assignments.values().any(|entity| {
                world
                    .get::<ActiveAssignment>(*entity)
                    .is_some_and(|assignment| {
                        matches!(
                            assignment.target,
                            AssignmentTarget::OwnArmy(target)
                                | AssignmentTarget::ArmyToProvince(target, _)
                                if target == army
                        )
                    })
            })
        })
}

/// Whether a ship is committed to an assignment that names it explicitly.
fn ship_has_active_assignment(world: &World, ship: ShipId) -> bool {
    world
        .get_resource::<AssignmentsIndex>()
        .is_some_and(|index| {
            index.assignments.values().any(|entity| {
                world
                    .get::<ActiveAssignment>(*entity)
                    .is_some_and(|assignment| {
                        matches!(
                            assignment.target,
                            AssignmentTarget::ShipToProvince(target, _) if target == ship
                        )
                    })
            })
        })
}

/// Validates a command against the current world.
///
/// Validation must be deterministic and side-effect free: replays re-run it.
pub fn validate_command(world: &World, command: &PlayerCommand) -> Result<(), CommandRejection> {
    match command {
        PlayerCommand::Noop => Ok(()),
        PlayerCommand::RenameCampaign { name } => {
            let length = name.chars().count();
            if (1..=120).contains(&length) {
                Ok(())
            } else {
                Err(CommandRejection::InvalidCampaignName { length })
            }
        }
        PlayerCommand::StartAssignment {
            assignment,
            leader,
            target,
        } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            assignments::validate_start(world, org, assignment, *leader, *target)?;
            Ok(())
        }
        PlayerCommand::StartSituationAssignment {
            situation,
            action,
            leader,
            target,
            war,
        } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            if crate::situations::action_war(situation, *target) != *war {
                return Err(crate::situations::SituationError::BadSubject(
                    "action's formal-war identity differs from its current projection".to_owned(),
                )
                .into());
            }
            let assignment = crate::situations::assignment_for_action(
                world, situation, action, *leader, *target,
            )?;
            assignments::validate_start_in_war(world, org, &assignment, *leader, *target, *war)?;
            Ok(())
        }
        PlayerCommand::DismissSituationResolution { resolution } => {
            world
                .get_resource::<PlayerHouse>()
                .and_then(|player| player.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            let visible = world
                .get_resource::<crate::situations::SituationState>()
                .and_then(|state| {
                    state
                        .resolutions
                        .iter()
                        .find(|notice| notice.id == *resolution)
                })
                .is_some_and(|notice| {
                    crate::situations::visible_to_player(world, &notice.situation)
                });
            if visible {
                Ok(())
            } else {
                Err(CommandRejection::BadSituationResolution)
            }
        }
        PlayerCommand::CancelAssignment { assignment } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            let owned = crate::access::assignment(world, *assignment)
                .is_some_and(|active| active.owner == org);
            if owned {
                Ok(())
            } else {
                Err(AssignmentRejection::BadAssignment.into())
            }
        }
        PlayerCommand::AnswerPopup { popup, choice } => {
            let valid = world
                .get_resource::<crate::assignments::PendingPopups>()
                .is_some_and(|popups| {
                    popups
                        .popups
                        .iter()
                        .any(|p| p.id == *popup && p.choices.iter().any(|(id, _)| id == choice))
                });
            if valid {
                Ok(())
            } else {
                Err(AssignmentRejection::BadPopupAnswer.into())
            }
        }
        PlayerCommand::Travel {
            character,
            destination,
        } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            let member = crate::access::character(world, *character)
                .is_some_and(|r| r.alive() && r.organisation == Some(org));
            if !member {
                return Err(AssignmentRejection::IneligibleLeader.into());
            }
            match presence::character_location(world, *character) {
                Some(Location::Province(at)) if at != *destination => {
                    let known = crate::access::province_entity(world, *destination).is_some();
                    if known {
                        Ok(())
                    } else {
                        Err(AssignmentRejection::BadTarget.into())
                    }
                }
                _ => Err(AssignmentRejection::BadTarget.into()),
            }
        }
        PlayerCommand::MoveShip { ship, destination } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            let ok = crate::access::ship(world, *ship).is_some_and(|s| {
                s.owner == org
                    && matches!(
                        s.location,
                        crate::forces::ShipLocation::Docked(at) if at != *destination
                    )
            }) && crate::access::province_entity(world, *destination).is_some();
            if ok && !ship_has_active_assignment(world, *ship) {
                Ok(())
            } else if ok {
                Err(CommandRejection::ForceCommitted)
            } else {
                Err(AssignmentRejection::BadTarget.into())
            }
        }
        PlayerCommand::DisbandArmy { army } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            let owned = crate::access::army(world, *army).is_some_and(|a| a.owner == org);
            if owned && !army_has_active_assignment(world, *army) {
                Ok(())
            } else if owned {
                Err(CommandRejection::ForceCommitted)
            } else {
                Err(AssignmentRejection::BadAssignment.into())
            }
        }
        PlayerCommand::SetShipCaptain { ship, captain } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            let Some(record) = crate::access::ship(world, *ship) else {
                return Err(AssignmentRejection::BadTarget.into());
            };
            if record.owner != org {
                return Err(AssignmentRejection::BadTarget.into());
            }
            // Repeating the current posting is harmless. Any actual change
            // waits until an assignment using this ship has finished: its
            // original captain and hull are part of the operation's contract.
            if record.captain == *captain {
                return Ok(());
            }
            if ship_has_active_assignment(world, *ship) {
                return Err(CommandRejection::ForceCommitted);
            }
            // Taking command requires an officer free to hold it: a standing
            // command elsewhere, active work, or indisposition all bar it.
            let Some(captain) = captain else {
                return Ok(());
            };
            let date = world.resource::<CampaignClock>().date;
            match assignments::leader_availability(world, org, *captain, date) {
                assignments::LeaderAvailability::Available => Ok(()),
                // Already this ship's captain: a harmless no-op.
                assignments::LeaderAvailability::Posted(assignments::Post::Captain {
                    ship: held,
                    ..
                }) if held == *ship => Ok(()),
                assignments::LeaderAvailability::Posted(_) => {
                    Err(AssignmentRejection::AlreadyAssigned.into())
                }
                assignments::LeaderAvailability::Busy { .. } => {
                    Err(AssignmentRejection::LeaderBusy.into())
                }
                assignments::LeaderAvailability::Indisposed { .. } => {
                    Err(AssignmentRejection::LeaderIndisposed.into())
                }
                assignments::LeaderAvailability::Ineligible(rejection) => Err(rejection.into()),
            }
        }
        PlayerCommand::SetStandingOrders { army, .. } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            let owned = crate::access::army(world, *army).is_some_and(|a| a.owner == org);
            if owned {
                Ok(())
            } else {
                Err(AssignmentRejection::BadAssignment.into())
            }
        }
        // A directive is authorised by the chain of command, not by
        // ownership: the player may press one on a house that answers
        // directly to them, and on no other.
        PlayerCommand::IssueDirective { vassal, .. } | PlayerCommand::ClearDirective { vassal } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            if crate::politics::answers_to(world, *vassal, org) == Some(1) {
                Ok(())
            } else {
                Err(CommandRejection::NotYourVassal)
            }
        }
        PlayerCommand::SetTradeRoute { ship, .. } | PlayerCommand::ClearTradeRoute { ship } => {
            let org = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .ok_or(AssignmentRejection::NoPlayerOrg)?;
            let owned = crate::access::ship(world, *ship).is_some_and(|s| s.owner == org);
            if owned {
                Ok(())
            } else {
                Err(AssignmentRejection::BadAssignment.into())
            }
        }
    }
}

/// Applies a single command's effects to the world.
fn apply_command(world: &mut World, command: &PlayerCommand) {
    match command {
        PlayerCommand::Noop => {}
        PlayerCommand::RenameCampaign { name } => {
            world.resource_mut::<CampaignMeta>().name = name.clone();
        }
        PlayerCommand::StartAssignment {
            assignment,
            leader,
            target,
        } => {
            // Conditions may have changed since submission; re-validate
            // and drop silently if the start is no longer legal (the
            // command log still records the attempt deterministically).
            if let Some(org) = world.get_resource::<PlayerHouse>().and_then(|p| p.0)
                && assignments::validate_start(world, org, assignment, *leader, *target).is_ok()
            {
                assignments::start_assignment(world, org, assignment, *leader, *target);
            }
        }
        PlayerCommand::StartSituationAssignment {
            situation,
            action,
            leader,
            target,
            war,
        } => {
            if let Some(org) = world.get_resource::<PlayerHouse>().and_then(|p| p.0)
                && crate::situations::action_war(situation, *target) == *war
                && let Ok(assignment) = crate::situations::assignment_for_action(
                    world, situation, action, *leader, *target,
                )
                && assignments::validate_start_in_war(
                    world,
                    org,
                    &assignment,
                    *leader,
                    *target,
                    *war,
                )
                .is_ok()
            {
                assignments::start_assignment_from_situation(
                    world,
                    org,
                    &assignment,
                    *leader,
                    *target,
                    situation.clone(),
                );
            }
        }
        PlayerCommand::DismissSituationResolution { resolution } => {
            let visible = world
                .get_resource::<PlayerHouse>()
                .and_then(|player| player.0)
                .is_some()
                && world
                    .get_resource::<crate::situations::SituationState>()
                    .and_then(|state| {
                        state
                            .resolutions
                            .iter()
                            .find(|notice| notice.id == *resolution)
                    })
                    .is_some_and(|notice| {
                        crate::situations::visible_to_player(world, &notice.situation)
                    });
            if visible {
                crate::situations::dismiss_resolution(world, *resolution);
            }
        }
        PlayerCommand::CancelAssignment { assignment } => {
            crate::assignments::request_cancel(world, *assignment);
        }
        PlayerCommand::AnswerPopup { popup, choice } => {
            let _ = assignments::answer_popup(world, *popup, choice);
        }
        PlayerCommand::Travel {
            character,
            destination,
        } => {
            presence::begin_travel(world, *character, *destination);
        }
        PlayerCommand::MoveShip { ship, destination } => {
            if validate_command(world, command).is_err() {
                return;
            }
            let date = world.resource::<CampaignClock>().date;
            let entity = crate::access::ship_entity(world, *ship);
            if let Some(entity) = entity {
                let from = match world
                    .get::<crate::forces::ShipRecord>(entity)
                    .map(|s| s.location)
                {
                    Some(crate::forces::ShipLocation::Docked(at)) => Some(at),
                    _ => None,
                };
                if let Some(from) = from {
                    // Ships cross space a third faster than liners.
                    let days = (presence::travel_days(world, from, *destination) * 2 / 3).max(2);
                    if let Some(mut ship_record) =
                        world.get_mut::<crate::forces::ShipRecord>(entity)
                    {
                        ship_record.blockading = None;
                        ship_record.location = crate::forces::ShipLocation::Transit {
                            to: *destination,
                            arrives: date.add_days(days),
                        };
                    }
                }
            }
        }
        PlayerCommand::DisbandArmy { army } => {
            if validate_command(world, command).is_ok() {
                crate::forces::disband_army(world, *army);
            }
        }
        PlayerCommand::SetShipCaptain { ship, captain } => {
            if validate_command(world, command).is_ok()
                && let Some(entity) = crate::access::ship_entity(world, *ship)
                && let Some(mut record) = world.get_mut::<crate::forces::ShipRecord>(entity)
            {
                record.captain = *captain;
                if captain.is_none() {
                    record.blockading = None;
                }
            }
        }
        PlayerCommand::SetStandingOrders { army, orders } => {
            if let Some(entity) = crate::access::army_entity(world, *army)
                && let Some(mut record) = world.get_mut::<crate::forces::ArmyRecord>(entity)
            {
                record.standing_order = orders.clone();
            }
        }
        PlayerCommand::IssueDirective {
            vassal,
            intent,
            target,
        } => {
            // Re-check authority: the hierarchy may have shifted since
            // submission. The log still records the attempt.
            if let Some(org) = world.get_resource::<PlayerHouse>().and_then(|p| p.0)
                && crate::politics::answers_to(world, *vassal, org) == Some(1)
            {
                world
                    .resource_mut::<crate::goals::IssuedDirectives>()
                    .by_vassal
                    .insert(
                        *vassal,
                        crate::goals::IssuedDirective {
                            from: org,
                            intent: *intent,
                            target: *target,
                        },
                    );
            }
        }
        PlayerCommand::ClearDirective { vassal } => {
            if let Some(mut issued) = world.get_resource_mut::<crate::goals::IssuedDirectives>() {
                issued.by_vassal.remove(vassal);
            }
        }
        PlayerCommand::SetTradeRoute { ship, route } => {
            // Re-check ownership; the route is set only if it still makes
            // sense (an owned transport, two different worlds).
            if let Some(org) = world.get_resource::<PlayerHouse>().and_then(|p| p.0)
                && crate::access::ship(world, *ship).is_some_and(|s| s.owner == org)
            {
                crate::trade::set_route(world, *ship, route.clone());
            }
        }
        PlayerCommand::ClearTradeRoute { ship } => {
            crate::trade::clear_route(world, *ship);
        }
    }
}

/// Submits a player command into a campaign world: validate, assign the
/// next day and sequence number, and queue it. Shared by the headless
/// host and the interactive client so both record identical logs.
pub fn submit_command(
    world: &mut World,
    command: PlayerCommand,
) -> Result<CommandEnvelope, CommandRejection> {
    validate_command(world, &command)?;
    let actor = match &command {
        PlayerCommand::StartAssignment { leader, .. }
        | PlayerCommand::StartSituationAssignment { leader, .. } => Some(*leader),
        PlayerCommand::Travel { character, .. } => Some(*character),
        _ => None,
    };
    let delay = presence::order_delay(world, actor);
    let day = world.resource::<CampaignClock>().date.add_days(1 + delay);
    let seq = {
        let mut log = world.resource_mut::<CommandLog>();
        let seq = log.next_seq;
        log.next_seq += 1;
        seq
    };
    let envelope = CommandEnvelope { seq, day, command };
    world
        .resource_mut::<PendingCommands>()
        .insert(envelope.clone());
    Ok(envelope)
}

/// Applies every command due this tick, in `(day, seq)` order.
fn apply_due_commands(world: &mut World) {
    let date = world.resource::<CampaignClock>().date;
    let due = world.resource_mut::<PendingCommands>().take_due(date);
    for envelope in due {
        apply_command(world, &envelope.command);
        world.resource_mut::<CommandLog>().applied.push(envelope);
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(DailyTick, apply_due_commands.in_set(TickSet::Commands));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(seq: u64, day: i64) -> CommandEnvelope {
        CommandEnvelope {
            seq,
            day: GameDate::from_days(day),
            command: PlayerCommand::Noop,
        }
    }

    #[test]
    fn pending_commands_keep_day_then_seq_order() {
        let mut pending = PendingCommands::default();
        pending.insert(envelope(3, 5));
        pending.insert(envelope(1, 2));
        pending.insert(envelope(2, 5));
        let due = pending.take_due(GameDate::from_days(5));
        let order: Vec<u64> = due.iter().map(|e| e.seq).collect();
        assert_eq!(order, vec![1, 2, 3]);
    }

    #[test]
    fn take_due_leaves_future_commands_queued() {
        let mut pending = PendingCommands::default();
        pending.insert(envelope(1, 2));
        pending.insert(envelope(2, 9));
        let due = pending.take_due(GameDate::from_days(5));
        assert_eq!(due.len(), 1);
        assert_eq!(pending.entries().len(), 1);
        assert_eq!(pending.entries()[0].seq, 2);
    }

    #[test]
    fn envelopes_serialise_to_stable_json() {
        let env = CommandEnvelope {
            seq: 4,
            day: GameDate::from_days(12),
            command: PlayerCommand::RenameCampaign {
                name: "House Veyrin Ascendant".to_owned(),
            },
        };
        let json = serde_json::to_string(&env).unwrap();
        assert_eq!(
            json,
            r#"{"seq":4,"day":12,"command":{"type":"rename-campaign","name":"House Veyrin Ascendant"}}"#
        );
        let back: CommandEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
    }
}
