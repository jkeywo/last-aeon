//! Who you could put to work: household members standing idle, and those on
//! an assignment of yours you could call off to free them.
//!
//! The two questions a player asks when an order needs giving — "who is
//! free?" and "who could I free?" — answered in one list, rather than hunted
//! for one selection at a time in the inspector. Choosing an action for
//! anyone here opens the same assignment popup the inspector does, because it
//! *is* the same action: nothing is decided here that is not decided there.

use aeon_sim::{ActiveAssignment, CharacterId, LeaderAvailability, PlayerCommand};
use bevy_egui::egui;

use crate::ui::actions::{AssignmentScope, draw_context_assignments};
use crate::ui::panel::{PanelCtx, PanelOut};

/// One household member and the state that decides which list they belong in.
struct Member {
    id: CharacterId,
    name: String,
    /// The assignment they lead for us, if any: its id, whether it can still
    /// be recalled, and whether a recall is already pending.
    posting: Option<Posting>,
}

/// A player-owned assignment a member is currently leading.
struct Posting {
    assignment: aeon_sim::AssignmentId,
    recallable: bool,
    cancel_requested: bool,
}

/// Draws the idle panel: the unassigned above, the interruptible below.
pub fn draw_idle_panel(ui: &mut egui::Ui, ctx: &PanelCtx, out: &mut PanelOut) {
    let strings = ctx.strings;
    let Some(player_org) = ctx.player_org else {
        ui.label(strings.text("ui.panel.idle.none"));
        return;
    };

    // The living household, sorted by name so the list never wobbles between
    // frames.
    let mut members: Vec<Member> = ctx
        .politics
        .characters
        .iter()
        .filter_map(|(id, entity)| {
            let (record, ..) = ctx.data.characters.get(*entity).ok()?;
            if !(record.alive() && record.organisation == Some(player_org)) {
                return None;
            }
            let posting = ctx
                .data
                .active_assignments
                .iter()
                .find(|a| a.owner == player_org && a.leader == *id)
                .map(|active| posting_of(active, ctx));
            Some(Member {
                id: *id,
                name: record.name.clone(),
                posting,
            })
        })
        .collect();
    members.sort_by(|a, b| a.name.cmp(&b.name));

    // Unassigned: leading nothing of ours and free to take something on.
    // Interruptible: leading one of our assignments we could call off.
    // Anyone busy elsewhere or indisposed is neither, and is left out.
    let unassigned: Vec<&Member> = members
        .iter()
        .filter(|m| {
            m.posting.is_none()
                && matches!(
                    ctx.data.availability.of(m.id),
                    Some(LeaderAvailability::Available)
                )
        })
        .collect();
    let interruptible: Vec<&Member> = members.iter().filter(|m| m.posting.is_some()).collect();

    if unassigned.is_empty() && interruptible.is_empty() {
        ui.label(strings.text("ui.panel.idle.none"));
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("idle-scroll")
        .show(ui, |ui| {
            if !unassigned.is_empty() {
                ui.strong(strings.text("ui.panel.idle.unassigned"));
                for member in &unassigned {
                    draw_member(ui, ctx, out, member);
                }
            }
            if !interruptible.is_empty() {
                ui.separator();
                ui.strong(strings.text("ui.panel.idle.interruptible"));
                for member in &interruptible {
                    draw_member(ui, ctx, out, member);
                }
            }
        });
}

/// One member's row: a collapsing header whose body is their assignment
/// actions, plus a recall control when they are leading one of ours.
fn draw_member(ui: &mut egui::Ui, ctx: &PanelCtx, out: &mut PanelOut, member: &Member) {
    let strings = ctx.strings;
    egui::CollapsingHeader::new(&member.name)
        .id_salt(("idle-member", member.id))
        .show(ui, |ui| {
            if let Some(posting) = &member.posting {
                if posting.cancel_requested {
                    ui.weak(strings.text("ui.assignments.cancel-pending"));
                } else if ui
                    .add_enabled(
                        posting.recallable,
                        egui::Button::new(strings.text("ui.assignments.cancel")).small(),
                    )
                    .on_disabled_hover_text(strings.text("ui.assignments.cannot-recall"))
                    .clicked()
                {
                    out.queue.0.push(PlayerCommand::CancelAssignment {
                        assignment: posting.assignment,
                    });
                }
            }
            // The same context actions the inspector shows for a member of
            // ours, so choosing one opens the assignment popup exactly as it
            // does there.
            draw_context_assignments(
                ui,
                AssignmentScope::OwnCharacter(member.id),
                ctx.content,
                ctx.data,
                ctx.player_org.expect("checked by the caller"),
                ctx.player_head,
                out.form,
                out.popup,
            );
        });
}

/// The recall state of an assignment a member is leading.
fn posting_of(active: &ActiveAssignment, ctx: &PanelCtx) -> Posting {
    let def = ctx.content.assignments.get(&active.def);
    Posting {
        assignment: active.id,
        recallable: def.is_some_and(|def| active.interruptible_on(def, ctx.date)),
        cancel_requested: active.cancel_requested,
    }
}
