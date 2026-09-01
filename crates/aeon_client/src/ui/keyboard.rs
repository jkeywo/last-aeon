//! Rendered-order keyboard policy for the presentation layer.
//!
//! Egui widgets are produced in implementation order, but the campaign shell
//! is not painted in that order (notably right-to-left top chrome and the
//! Bottom/Left/Right panel construction). This module records the rectangles
//! that were actually rendered, sorts them into visual surface bands, and owns
//! Tab/Shift+Tab before egui's registration-order traversal.

use std::cmp::Ordering;

use bevy_egui::egui;

const REGISTRY_STATE: &str = "rendered-keyboard-registry";

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FocusBand {
    TopChrome,
    Overlay,
    Floating,
    Left,
    Center,
    Right,
    BottomTabs,
    BottomBody,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogicalFocus(pub String);

impl LogicalFocus {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // geometry/layer fields are also consumed by rendered evidence
pub struct FocusEntry {
    pub logical: LogicalFocus,
    pub role: &'static str,
    pub id: egui::Id,
    pub band: FocusBand,
    pub layer: egui::LayerId,
    pub rect: egui::Rect,
    pub clip: egui::Rect,
    /// Stable lifecycle identity used only for Situation action-to-resolution
    /// repair. Keeping this structural avoids guessing from display labels or
    /// selecting an unrelated newest notice.
    pub situation_occurrence: Option<String>,
    pub situation_action: Option<String>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // read by native/wasm rendered-state audit evidence
pub struct AuditedResponse {
    pub id: egui::Id,
    pub logical: LogicalFocus,
    pub role: &'static str,
    pub band: FocusBand,
    pub layer: egui::LayerId,
    pub rect: egui::Rect,
    pub enabled: bool,
}

#[derive(Clone, Default)]
struct RegistryState {
    pass: u64,
    previous: Vec<FocusEntry>,
    current: Vec<FocusEntry>,
    logical_focus: Option<LogicalFocus>,
    pending: Option<LogicalFocus>,
    fallback: Option<LogicalFocus>,
    audited: Vec<AuditedResponse>,
}

fn state_id() -> egui::Id {
    egui::Id::new(REGISTRY_STATE)
}

fn compare_entries(left: &FocusEntry, right: &FocusEntry) -> Ordering {
    left.band
        .cmp(&right.band)
        .then_with(|| (left.layer.order as u8).cmp(&(right.layer.order as u8)))
        .then_with(|| left.rect.top().round().total_cmp(&right.rect.top().round()))
        .then_with(|| left.rect.left().total_cmp(&right.rect.left()))
        .then_with(|| left.rect.bottom().total_cmp(&right.rect.bottom()))
        .then_with(|| left.role.cmp(right.role))
        .then_with(|| left.logical.cmp(&right.logical))
}

fn sorted(mut entries: Vec<FocusEntry>) -> Vec<FocusEntry> {
    entries.sort_by(compare_entries);
    let mut seen = std::collections::BTreeSet::new();
    entries.retain(|entry| seen.insert(entry.logical.clone()));
    entries
}

/// Begin a registry pass and consume Tab before egui's global traversal.
pub fn begin_frame(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    let focused = ctx.memory(|memory| memory.focused());
    let (tab, backwards) = ctx.input_mut(|input| {
        let backwards = input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
        let forwards = input.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
        (backwards || forwards, backwards)
    });
    let mut state = ctx.data_mut(|data| {
        data.get_temp::<RegistryState>(state_id())
            .unwrap_or_default()
    });
    if state.pass != pass {
        state.previous = sorted(std::mem::take(&mut state.current));
        state.audited.clear();
        state.pass = pass;
    }
    if let (Some(pending), Some(id)) = (state.pending.as_ref(), focused)
        && state
            .previous
            .iter()
            .any(|entry| &entry.logical == pending && entry.id == id)
    {
        state.logical_focus = Some(pending.clone());
        state.pending = None;
        state.fallback = None;
    }
    if state.pending.is_none()
        && let Some(id) = focused
        && let Some(entry) = state.previous.iter().find(|entry| entry.id == id)
    {
        state.logical_focus = Some(entry.logical.clone());
    }
    if tab && !state.previous.is_empty() {
        let index = state.logical_focus.as_ref().and_then(|logical| {
            state
                .previous
                .iter()
                .position(|entry| &entry.logical == logical)
        });
        let is_floating = |entry: &FocusEntry| entry.band == FocusBand::Floating;
        let focused_is_floating = index.is_some_and(|index| is_floating(&state.previous[index]));
        let next = if !focused_is_floating {
            let candidates = state
                .previous
                .iter()
                .enumerate()
                .filter(|(_, entry)| is_floating(entry));
            if backwards {
                candidates.map(|(index, _)| index).next_back()
            } else {
                candidates.map(|(index, _)| index).next()
            }
        } else {
            None
        }
        .unwrap_or_else(|| match (index, backwards) {
            (Some(index), true) => (index + state.previous.len() - 1) % state.previous.len(),
            (Some(index), false) => (index + 1) % state.previous.len(),
            (None, true) => state.previous.len() - 1,
            (None, false) => 0,
        });
        state.pending = Some(state.previous[next].logical.clone());
        state.logical_focus = state.pending.clone();
        // Most controls retain their egui ID across adjacent frames, so move
        // immediately before widgets/global spatial navigation run. The
        // logical pending target remains authoritative: `register` repeats
        // the request against a new ID after responsive reflow.
        ctx.memory_mut(|memory| memory.request_focus(state.previous[next].id));
    }
    ctx.data_mut(|data| data.insert_temp(state_id(), state));
}

fn register_with_situation(
    ui: &egui::Ui,
    logical: LogicalFocus,
    role: &'static str,
    band: FocusBand,
    response: &egui::Response,
    occurrence: Option<String>,
    action: Option<String>,
) {
    if !response.enabled() || !response.sense.is_focusable() || response.rect.is_negative() {
        return;
    }
    let mut state = ui.ctx().data_mut(|data| {
        data.get_temp::<RegistryState>(state_id())
            .unwrap_or_default()
    });
    let wants_focus = state.pending.as_ref() == Some(&logical);
    state.current.push(FocusEntry {
        logical: logical.clone(),
        role,
        id: response.id,
        band,
        layer: response.layer_id,
        rect: response.rect,
        clip: ui.clip_rect(),
        situation_occurrence: occurrence,
        situation_action: action,
    });
    if wants_focus && !response.has_focus() {
        response.request_focus();
        state.logical_focus = Some(logical);
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(state_id(), state));
}

/// A raw response already captured by the production action wrapper, but not
/// yet admitted to keyboard traversal. Registration is deliberately separate
/// so the audit can observe omissions.
#[must_use = "captured actions must be explicitly registered"]
pub struct CapturedAction<'a> {
    ui: &'a egui::Ui,
    logical: LogicalFocus,
    role: &'static str,
    band: FocusBand,
    response: &'a egui::Response,
    occurrence: Option<String>,
    action_key: Option<String>,
}

impl CapturedAction<'_> {
    /// Explicitly completes the independent raw-capture -> registry pipeline.
    pub fn register(self) {
        paint_focus(self.ui, self.response);
        register_with_situation(
            self.ui,
            self.logical,
            self.role,
            self.band,
            self.response,
            self.occurrence,
            self.action_key,
        );
    }
}

/// Capture an enabled raw production response before it can be registered.
/// Tests intentionally retain one of these without calling `register` to
/// prove the audit detects the omission.
pub fn capture_action<'a>(
    ui: &'a egui::Ui,
    logical: LogicalFocus,
    role: &'static str,
    band: FocusBand,
    response: &'a egui::Response,
) -> CapturedAction<'a> {
    audit(ui, logical.clone(), role, band, response);
    CapturedAction {
        ui,
        logical,
        role,
        band,
        response,
        occurrence: None,
        action_key: None,
    }
}

pub fn situation_action<'a>(
    ui: &'a egui::Ui,
    logical: LogicalFocus,
    occurrence: String,
    action_key: String,
    band: FocusBand,
    response: &'a egui::Response,
) -> CapturedAction<'a> {
    audit(ui, logical.clone(), "situation-action", band, response);
    CapturedAction {
        ui,
        logical,
        role: "situation-action",
        band,
        response,
        occurrence: Some(occurrence),
        action_key: Some(action_key),
    }
}

pub fn situation_resolution<'a>(
    ui: &'a egui::Ui,
    logical: LogicalFocus,
    occurrence: String,
    band: FocusBand,
    response: &'a egui::Response,
) -> CapturedAction<'a> {
    audit(ui, logical.clone(), "resolution-summary", band, response);
    CapturedAction {
        ui,
        logical,
        role: "resolution-summary",
        band,
        response,
        occurrence: Some(occurrence),
        action_key: None,
    }
}

fn audit(
    ui: &egui::Ui,
    logical: LogicalFocus,
    role: &'static str,
    band: FocusBand,
    response: &egui::Response,
) {
    if !response.sense.is_focusable() || response.rect.is_negative() {
        return;
    }
    let mut state = ui.ctx().data_mut(|data| {
        data.get_temp::<RegistryState>(state_id())
            .unwrap_or_default()
    });
    state.audited.push(AuditedResponse {
        id: response.id,
        logical,
        role,
        band,
        layer: response.layer_id,
        rect: response.rect,
        enabled: response.enabled(),
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(state_id(), state));
}

/// Infer a dock band from actual rendered geometry for reusable controls whose
/// caller does not know which responsive surface currently owns them.
pub fn inferred_band(ctx: &egui::Context, rect: egui::Rect) -> FocusBand {
    let viewport = ctx.viewport_rect();
    if rect.center().y <= viewport.top() + 140.0 {
        FocusBand::TopChrome
    } else if rect.center().y >= viewport.top() + viewport.height() * 0.68 {
        FocusBand::BottomBody
    } else if rect.center().x <= viewport.left() + viewport.width() * 0.34 {
        FocusBand::Left
    } else if rect.center().x >= viewport.left() + viewport.width() * 0.66 {
        FocusBand::Right
    } else {
        FocusBand::Center
    }
}

/// Request a stable logical target. If it disappeared, finish-frame repair
/// selects its next visible neighbour in the previous visual order.
pub fn request_logical(ctx: &egui::Context, logical: LogicalFocus) {
    let mut state = ctx.data_mut(|data| {
        data.get_temp::<RegistryState>(state_id())
            .unwrap_or_default()
    });
    state.pending = Some(logical.clone());
    state.fallback = state
        .previous
        .iter()
        .position(|entry| entry.logical == logical)
        .and_then(|index| {
            (!state.previous.is_empty()).then(|| {
                state.previous[(index + 1) % state.previous.len()]
                    .logical
                    .clone()
            })
        });
    ctx.data_mut(|data| data.insert_temp(state_id(), state));
}

fn finish(ctx: &egui::Context) {
    let mut state = ctx.data_mut(|data| {
        data.get_temp::<RegistryState>(state_id())
            .unwrap_or_default()
    });
    let focused_action = state.logical_focus.as_ref().and_then(|logical| {
        state
            .previous
            .iter()
            .find(|entry| &entry.logical == logical && entry.role == "situation-action")
    });
    let focused_action_disappeared = focused_action.is_some_and(|action| {
        !state
            .current
            .iter()
            .any(|entry| entry.logical == action.logical)
    });
    let matching_new_resolution = focused_action.and_then(|action| {
        let occurrence = action.situation_occurrence.as_ref()?;
        state.current.iter().find(|entry| {
            entry.role == "resolution-summary"
                && entry.situation_occurrence.as_ref() == Some(occurrence)
                && !state
                    .previous
                    .iter()
                    .any(|previous| previous.logical == entry.logical)
        })
    });
    if focused_action_disappeared && let Some(entry) = matching_new_resolution {
        ctx.memory_mut(|memory| memory.request_focus(entry.id));
        state.logical_focus = Some(entry.logical.clone());
        state.pending = None;
        state.fallback = None;
    }
    // Reconcile after every surface has registered. This is deliberately
    // later than egui's widget/global spatial-navigation decisions, so the
    // rendered registry is the final authority for the frame.
    if let Some(pending) = state.pending.as_ref()
        && let Some(entry) = state.current.iter().find(|entry| &entry.logical == pending)
    {
        ctx.memory_mut(|memory| memory.request_focus(entry.id));
        state.logical_focus = Some(entry.logical.clone());
    }
    if state.pending.is_none()
        && let Some(logical) = state.logical_focus.as_ref()
        && let Some(entry) = state.current.iter().find(|entry| &entry.logical == logical)
        && ctx.memory(|memory| memory.focused()) != Some(entry.id)
    {
        ctx.memory_mut(|memory| memory.request_focus(entry.id));
    }
    if state.pending.is_some()
        && !state
            .current
            .iter()
            .any(|entry| Some(&entry.logical) == state.pending.as_ref())
    {
        let missing = state.pending.as_ref().and_then(|logical| {
            state
                .previous
                .iter()
                .find(|entry| &entry.logical == logical)
        });
        let replacement = state
            .fallback
            .as_ref()
            .and_then(|fallback| {
                state.current.iter().find(|entry| {
                    &entry.logical == fallback
                        && missing.is_none_or(|old| repair_candidate(old, entry))
                })
            })
            .or_else(|| {
                missing.and_then(|old| {
                    state
                        .current
                        .iter()
                        .filter(|entry| entry.band == old.band && repair_candidate(old, entry))
                        .min_by(|left, right| {
                            left.rect
                                .center()
                                .distance_sq(old.rect.center())
                                .total_cmp(&right.rect.center().distance_sq(old.rect.center()))
                        })
                })
            })
            .or_else(|| {
                state
                    .current
                    .iter()
                    .find(|entry| missing.is_none_or(|old| repair_candidate(old, entry)))
            });
        if let Some(entry) = replacement {
            ctx.memory_mut(|memory| memory.request_focus(entry.id));
            state.logical_focus = Some(entry.logical.clone());
            state.pending = None;
            state.fallback = None;
        }
    }
    if state.pending.is_none()
        && let Some(logical) = state.logical_focus.clone()
        && !state.current.iter().any(|entry| entry.logical == logical)
        && let Some(old) = state.previous.iter().find(|entry| entry.logical == logical)
    {
        let nearest = state
            .current
            .iter()
            .filter(|entry| entry.band == old.band && repair_candidate(old, entry))
            .min_by(|left, right| {
                left.rect
                    .center()
                    .distance_sq(old.rect.center())
                    .total_cmp(&right.rect.center().distance_sq(old.rect.center()))
            })
            .or_else(|| {
                state
                    .current
                    .iter()
                    .find(|entry| repair_candidate(old, entry))
            });
        if let Some(entry) = nearest {
            ctx.memory_mut(|memory| memory.request_focus(entry.id));
            state.logical_focus = Some(entry.logical.clone());
        }
    }
    ctx.data_mut(|data| data.insert_temp(state_id(), state));
}

fn repair_candidate(old: &FocusEntry, candidate: &FocusEntry) -> bool {
    old.role != "situation-action" || candidate.role != "resolution-summary"
}

pub fn finish_frame(mut contexts: bevy_egui::EguiContexts) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    finish(ctx);
}

#[cfg(test)]
pub fn completed_registry(ctx: &egui::Context) -> Vec<FocusEntry> {
    ctx.data(|data| {
        data.get_temp::<RegistryState>(state_id())
            .map(|state| sorted(state.current))
            .unwrap_or_default()
    })
}

#[cfg(test)]
pub fn audit_gaps(ctx: &egui::Context) -> Vec<AuditedResponse> {
    ctx.data(|data| {
        let state = data
            .get_temp::<RegistryState>(state_id())
            .unwrap_or_default();
        let registry = sorted(state.current.clone());
        state
            .audited
            .iter()
            .filter(|audit| {
                audit.enabled
                    && !registry.iter().any(|entry| {
                        entry.id == audit.id
                            && entry.logical == audit.logical
                            && entry.role == audit.role
                    })
            })
            .cloned()
            .collect()
    })
}

#[cfg(test)]
pub fn audited_responses(ctx: &egui::Context) -> Vec<AuditedResponse> {
    ctx.data(|data| {
        data.get_temp::<RegistryState>(state_id())
            .map(|state| state.audited)
            .unwrap_or_default()
    })
}

#[cfg(test)]
pub fn traversal_registry(ctx: &egui::Context) -> Vec<FocusEntry> {
    ctx.data(|data| {
        data.get_temp::<RegistryState>(state_id())
            .map(|state| state.previous)
            .unwrap_or_default()
    })
}

#[cfg(test)]
pub fn logical_focus(ctx: &egui::Context) -> Option<LogicalFocus> {
    ctx.data(|data| {
        data.get_temp::<RegistryState>(state_id())
            .and_then(|state| state.logical_focus)
    })
}

/// Ends one completed keyboard route before a rendered test begins an
/// independent pointer-only interaction in the same retained egui context.
#[cfg(test)]
pub fn clear_focus(ctx: &egui::Context) {
    let mut state = ctx.data_mut(|data| {
        data.get_temp::<RegistryState>(state_id())
            .unwrap_or_default()
    });
    state.logical_focus = None;
    state.pending = None;
    state.fallback = None;
    ctx.data_mut(|data| data.insert_temp(state_id(), state));
    if let Some(focused) = ctx.memory(|memory| memory.focused()) {
        ctx.memory_mut(|memory| memory.surrender_focus(focused));
    }
}

pub fn paint_focus(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() {
        response.scroll_to_me(Some(egui::Align::Center));
        let mut stroke = ui.visuals().selection.stroke;
        stroke.width = stroke.width.max(2.0);
        ui.painter().rect_stroke(
            response.rect,
            ui.visuals().widgets.active.corner_radius,
            stroke,
            egui::StrokeKind::Inside,
        );
    }
}

/// Disabled/nonfocusable members are skipped. Text edits never call this, so
/// arrows remain editing keys there. Consuming now prevents end-pass spatial
/// navigation from competing with the group decision.
pub fn roving_group(ui: &egui::Ui, responses: &[egui::Response]) {
    let mut responses = responses
        .iter()
        .filter(|response| response.enabled() && response.sense.is_focusable())
        .collect::<Vec<_>>();
    // A few production rows are deliberately painted right-to-left. Arrow
    // order follows their rendered geometry, never construction order.
    responses.sort_by(|left, right| {
        left.rect
            .top()
            .round()
            .total_cmp(&right.rect.top().round())
            .then_with(|| left.rect.left().total_cmp(&right.rect.left()))
    });
    let Some(index) = responses.iter().position(|response| response.has_focus()) else {
        return;
    };
    let backwards = ui.input_mut(|input| {
        input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
    });
    let forwards = ui.input_mut(|input| {
        input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
    });
    let target = if backwards {
        Some(responses[(index + responses.len() - 1) % responses.len()])
    } else if forwards {
        Some(responses[(index + 1) % responses.len()])
    } else {
        None
    };
    if let Some(target) = target {
        target.request_focus();
        let mut state = ui.ctx().data_mut(|data| {
            data.get_temp::<RegistryState>(state_id())
                .unwrap_or_default()
        });
        if let Some(logical) = state
            .current
            .iter()
            .find(|entry| entry.id == target.id)
            .map(|entry| entry.logical.clone())
        {
            state.logical_focus = Some(logical.clone());
            state.pending = Some(logical);
        }
        ui.ctx()
            .data_mut(|data| data.insert_temp(state_id(), state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn entry(
        logical: &str,
        role: &'static str,
        x: f32,
        occurrence: Option<&str>,
        action: Option<&str>,
    ) -> FocusEntry {
        let rect = egui::Rect::from_min_size(egui::pos2(x, 10.0), egui::vec2(20.0, 20.0));
        FocusEntry {
            logical: LogicalFocus::new(logical),
            role,
            id: egui::Id::new(logical),
            band: FocusBand::Right,
            layer: egui::LayerId::new(egui::Order::Middle, egui::Id::new("test-layer")),
            rect,
            clip: egui::Rect::EVERYTHING,
            situation_occurrence: occurrence.map(str::to_owned),
            situation_action: action.map(str::to_owned),
        }
    }

    fn finish_with(
        ctx: &egui::Context,
        focused: FocusEntry,
        mut previous: Vec<FocusEntry>,
        current: Vec<FocusEntry>,
    ) -> LogicalFocus {
        previous.insert(0, focused.clone());
        ctx.data_mut(|data| {
            data.insert_temp(
                state_id(),
                RegistryState {
                    previous,
                    current,
                    logical_focus: Some(focused.logical),
                    ..Default::default()
                },
            );
        });
        finish(ctx);
        logical_focus(ctx).expect("repair leaves a logical focus")
    }

    #[test]
    fn situation_resolution_repair_is_occurrence_exact_and_never_steals() {
        let ctx = egui::Context::default();
        let action = entry(
            "action:a:answer",
            "situation-action",
            10.0,
            Some("occurrence-a"),
            Some("answer"),
        );
        let matching = entry(
            "resolution:a",
            "resolution-summary",
            20.0,
            Some("occurrence-a"),
            None,
        );
        let unrelated = entry(
            "resolution:b",
            "resolution-summary",
            30.0,
            Some("occurrence-b"),
            None,
        );
        let fallback = entry("right:fallback", "dock-control", 40.0, None, None);

        assert_eq!(
            finish_with(
                &ctx,
                action.clone(),
                vec![],
                vec![
                    action.clone(),
                    matching.clone(),
                    unrelated.clone(),
                    fallback.clone()
                ]
            ),
            action.logical,
            "a newly rendered resolution cannot steal focus while its action remains"
        );
        assert_eq!(
            finish_with(
                &ctx,
                action.clone(),
                vec![],
                vec![unrelated.clone(), matching.clone(), fallback.clone()]
            ),
            matching.logical,
            "the disappearing action repairs to its exact newly created occurrence"
        );
        assert_eq!(
            finish_with(&ctx, action, vec![], vec![unrelated, fallback.clone()]),
            fallback.logical,
            "an unrelated resolution is never selected as the action fallback"
        );
    }

    #[test]
    fn roving_group_skips_disabled_wraps_and_leaves_text_edit_arrows_alone() {
        let ctx = egui::Context::default();
        let draw = |events: Vec<egui::Event>, focus: Option<&str>| {
            let mut ids = [egui::Id::NULL; 3];
            let _ = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    let first = ui.button("first");
                    let disabled = ui.add_enabled(false, egui::Button::new("disabled"));
                    let last = ui.button("last");
                    let mut text = "edit me".to_owned();
                    let edit = ui.text_edit_singleline(&mut text);
                    ids = [first.id, last.id, edit.id];
                    match focus {
                        Some("first") => first.request_focus(),
                        Some("edit") => edit.request_focus(),
                        _ => {}
                    }
                    roving_group(ui, &[first, disabled, last]);
                },
            );
            ids
        };

        let ids = draw(Vec::new(), Some("first"));
        draw(vec![key(egui::Key::ArrowRight)], None);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(ids[1]));
        draw(vec![key(egui::Key::ArrowRight)], None);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(ids[0]));

        let ids = draw(Vec::new(), Some("edit"));
        draw(vec![key(egui::Key::ArrowLeft)], None);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(ids[2]));
    }

    #[test]
    fn raw_capture_audit_detects_an_enabled_action_omitted_from_registry() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(Default::default(), |ui| {
            begin_frame(ui.ctx());
            let response = ui.button("deliberately omitted");
            let _captured = capture_action(
                ui,
                LogicalFocus::new("negative:omitted"),
                "negative-action",
                FocusBand::Center,
                &response,
            );
        });
        let gaps = audit_gaps(&ctx);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].logical, LogicalFocus::new("negative:omitted"));

        let ctx = egui::Context::default();
        let _ = ctx.run_ui(Default::default(), |ui| {
            begin_frame(ui.ctx());
            let response = ui.button("registered");
            capture_action(
                ui,
                LogicalFocus::new("negative:registered"),
                "negative-action",
                FocusBand::Center,
                &response,
            )
            .register();
        });
        assert!(audit_gaps(&ctx).is_empty());
    }
}
