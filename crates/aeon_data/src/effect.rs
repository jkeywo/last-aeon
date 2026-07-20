//! The script effect boundary.
//!
//! Authored functions never mutate simulation state. They return an array
//! of effect maps — plain data — which Rust parses into [`ScriptEffect`]
//! values, validates, and applies. Returning effects (rather than calling
//! emitter functions) keeps script invocation pure: no shared collector
//! state, nothing to reset between calls, nothing order-dependent beyond
//! the returned array itself.

use rhai::Dynamic;

/// A typed effect emitted by an authored script function.
///
/// Variants grow as milestones add systems that scripts may affect. Every
/// variant is applied by Rust simulation code; scripts only describe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptEffect {
    /// Append a message to the notable-result log.
    Log {
        /// The string-table row holding the line.
        ///
        /// A script function returns effects from inside its own body,
        /// where there is no definition ID to derive a key from — so
        /// unlike a definition's prose, this key is authored.
        message_key: String,
    },
    /// Add a directional opinion modifier between job-context roles.
    Opinion {
        /// Role whose opinion changes.
        from: EffectRole,
        /// Role the opinion is about.
        toward: EffectRole,
        /// Signed amount.
        amount: i32,
        /// Days until expiry; `None` is permanent.
        days: Option<i64>,
        /// Stable reason tag.
        reason: String,
    },
    /// Form a persistent army at the job leader's location, spending the
    /// stated manpower and supplies from the owning organisation.
    FormArmy {
        /// Soldiers drawn from the organisation's manpower pool.
        manpower: i64,
        /// Supplies committed from the organisation's stores.
        supplies: i64,
    },
    /// Press the owner's claim to the vacant paramountcy. The simulation
    /// validates the claim: the title must be vacant and the owner must
    /// hold strictly more planetary provinces than any rival.
    ClaimParamountcy,
    /// Collect Imperial tithes: every house pays a twentieth of its
    /// wealth to the owner. Valid only for the Sanctora Imperim.
    CollectTithes,
    /// Create or settle a political obligation between the houses behind
    /// two job-context roles.
    Obligation {
        /// What to do to the ledger.
        action: ObligationAction,
        /// Which kind of obligation: favour, promise, or grievance.
        kind: crate::model::ObligationKind,
        /// Role whose house owes, or is resented.
        debtor: EffectRole,
        /// Role whose house is owed, or resents.
        creditor: EffectRole,
        /// How much it weighs.
        weight: i32,
        /// Days until it lapses; `None` never lapses.
        days: Option<i64>,
        /// Where it came from, in plain words.
        origin: String,
    },
    /// Change provincial order, either where the job acted or across
    /// every province the owner holds.
    Order {
        /// Which provinces the change reaches.
        scope: OrderScope,
        /// Signed change, in order points.
        amount: i32,
    },
}

/// A job-context role an authored effect may address.
///
/// These seven names are the whole vocabulary scripts have for naming
/// characters; the simulation resolves who actually stands behind each
/// role when the effect is applied. Parsing them here means a mistyped
/// role is a loud parse error instead of an effect that silently
/// addresses nobody.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EffectRole {
    /// The character leading the job.
    Leader,
    /// The character targeted, or the targeted organisation's head.
    Target,
    /// The head of the targeted organisation.
    TargetHead,
    /// The head of the organisation the job serves.
    OwnerHead,
    /// The head of the owner's liege organisation.
    LiegeHead,
    /// The character holding the Consul title.
    Consul,
    /// Every living member of the Sanctora Imperim.
    Sanctora,
}

impl EffectRole {
    /// Parses the authored spelling; anything else is a script error.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "leader" => Some(EffectRole::Leader),
            "target" => Some(EffectRole::Target),
            "target-head" => Some(EffectRole::TargetHead),
            "owner-head" => Some(EffectRole::OwnerHead),
            "liege-head" => Some(EffectRole::LiegeHead),
            "consul" => Some(EffectRole::Consul),
            "sanctora" => Some(EffectRole::Sanctora),
            _ => None,
        }
    }
}

/// What an [`ScriptEffect::Obligation`] does to the ledger.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ObligationAction {
    /// Record a new obligation.
    Create,
    /// Mark an existing one honoured.
    Fulfil,
    /// Mark an existing one repudiated.
    Break,
}

/// Which provinces an [`ScriptEffect::Order`] applies to.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OrderScope {
    /// The province the job targeted, or the leader's own location.
    TargetProvince,
    /// Every province the owning organisation holds.
    AllHeld,
}

/// Why a script's returned effects were rejected.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EffectParseError {
    /// The function returned something other than an array.
    #[error("effect functions must return an array of effect maps, got {type_name}")]
    NotAnArray {
        /// The Rhai type name actually returned.
        type_name: String,
    },
    /// An array element was not a map.
    #[error("effect #{index} is not a map")]
    NotAMap {
        /// Zero-based element index.
        index: usize,
    },
    /// An effect map is missing its `kind` field.
    #[error("effect #{index} has no 'kind' field")]
    MissingKind {
        /// Zero-based element index.
        index: usize,
    },
    /// An effect map names an unknown kind.
    #[error("effect #{index} has unknown kind '{kind}'")]
    UnknownKind {
        /// Zero-based element index.
        index: usize,
        /// The unrecognised kind.
        kind: String,
    },
    /// An effect map is missing or mistypes a required field.
    #[error("effect #{index} ({kind}): field '{field}' is missing or not a {expected}")]
    BadField {
        /// Zero-based element index.
        index: usize,
        /// The effect kind being parsed.
        kind: String,
        /// The offending field.
        field: String,
        /// The expected Rhai type.
        expected: String,
    },
}

/// Parses a script function's return value into typed effects.
pub fn parse_effects(value: Dynamic) -> Result<Vec<ScriptEffect>, EffectParseError> {
    // An empty return (unit) means "no effects" and is always fine.
    if value.is_unit() {
        return Ok(Vec::new());
    }
    let array = value
        .try_cast::<rhai::Array>()
        .ok_or_else(|| EffectParseError::NotAnArray {
            type_name: "non-array".to_owned(),
        })?;

    let mut effects = Vec::with_capacity(array.len());
    for (index, element) in array.into_iter().enumerate() {
        let map = element
            .try_cast::<rhai::Map>()
            .ok_or(EffectParseError::NotAMap { index })?;
        let kind = map
            .get("kind")
            .and_then(|k| k.clone().into_string().ok())
            .ok_or(EffectParseError::MissingKind { index })?;
        match kind.as_str() {
            "log" => {
                let message_key = map
                    .get("message_key")
                    .and_then(|m| m.clone().into_string().ok())
                    .ok_or_else(|| EffectParseError::BadField {
                        index,
                        kind: kind.clone(),
                        field: "message_key".to_owned(),
                        expected: "string".to_owned(),
                    })?;
                effects.push(ScriptEffect::Log { message_key });
            }
            "opinion" => {
                let get_role = |field: &str| -> Result<EffectRole, EffectParseError> {
                    map.get(field)
                        .and_then(|v| v.clone().into_string().ok())
                        .as_deref()
                        .and_then(EffectRole::parse)
                        .ok_or_else(|| EffectParseError::BadField {
                            index,
                            kind: kind.clone(),
                            field: field.to_owned(),
                            expected: "a role: leader, target, target-head, owner-head, \
                                       liege-head, consul, or sanctora"
                                .to_owned(),
                        })
                };
                let get_str = |field: &str| {
                    map.get(field)
                        .and_then(|v| v.clone().into_string().ok())
                        .ok_or_else(|| EffectParseError::BadField {
                            index,
                            kind: kind.clone(),
                            field: field.to_owned(),
                            expected: "string".to_owned(),
                        })
                };
                let amount = map
                    .get("amount")
                    .and_then(|v| v.as_int().ok())
                    .ok_or_else(|| EffectParseError::BadField {
                        index,
                        kind: kind.clone(),
                        field: "amount".to_owned(),
                        expected: "integer".to_owned(),
                    })?;
                let days = match map.get("days") {
                    None => None,
                    Some(v) => Some(v.as_int().map_err(|_| EffectParseError::BadField {
                        index,
                        kind: kind.clone(),
                        field: "days".to_owned(),
                        expected: "integer".to_owned(),
                    })?),
                };
                effects.push(ScriptEffect::Opinion {
                    from: get_role("from")?,
                    toward: get_role("toward")?,
                    amount: amount as i32,
                    days,
                    reason: get_str("reason")?,
                });
            }
            "claim-paramountcy" => {
                effects.push(ScriptEffect::ClaimParamountcy);
            }
            "collect-tithes" => {
                effects.push(ScriptEffect::CollectTithes);
            }
            "form-army" => {
                let get_int = |field: &str| {
                    map.get(field).and_then(|v| v.as_int().ok()).ok_or_else(|| {
                        EffectParseError::BadField {
                            index,
                            kind: kind.clone(),
                            field: field.to_owned(),
                            expected: "integer".to_owned(),
                        }
                    })
                };
                effects.push(ScriptEffect::FormArmy {
                    manpower: get_int("manpower")?,
                    supplies: get_int("supplies")?,
                });
            }
            "obligation" => {
                let get_str = |field: &str| -> Result<String, EffectParseError> {
                    map.get(field)
                        .and_then(|v| v.clone().into_string().ok())
                        .ok_or_else(|| EffectParseError::BadField {
                            index,
                            kind: kind.clone(),
                            field: field.to_owned(),
                            expected: "string".to_owned(),
                        })
                };
                let get_role = |field: &str| -> Result<EffectRole, EffectParseError> {
                    EffectRole::parse(&get_str(field)?).ok_or_else(|| EffectParseError::BadField {
                        index,
                        kind: kind.clone(),
                        field: field.to_owned(),
                        expected: "a role: leader, target, target-head, owner-head, \
                                       liege-head, consul, or sanctora"
                            .to_owned(),
                    })
                };
                let action = match get_str("action")?.as_str() {
                    "create" => ObligationAction::Create,
                    "fulfil" => ObligationAction::Fulfil,
                    "break" => ObligationAction::Break,
                    _ => {
                        return Err(EffectParseError::BadField {
                            index,
                            kind: kind.clone(),
                            field: "action".to_owned(),
                            expected: "\"create\", \"fulfil\" or \"break\"".to_owned(),
                        });
                    }
                };
                let obligation_kind = crate::model::ObligationKind::parse(&get_str("obligation")?)
                    .ok_or_else(|| EffectParseError::BadField {
                        index,
                        kind: kind.clone(),
                        field: "obligation".to_owned(),
                        expected: "\"favour\", \"promise\" or \"grievance\"".to_owned(),
                    })?;
                effects.push(ScriptEffect::Obligation {
                    action,
                    kind: obligation_kind,
                    debtor: get_role("debtor")?,
                    creditor: get_role("creditor")?,
                    weight: map
                        .get("weight")
                        .and_then(|v| v.as_int().ok())
                        .unwrap_or(20) as i32,
                    days: map.get("days").and_then(|v| v.as_int().ok()),
                    origin: map
                        .get("origin")
                        .and_then(|v| v.clone().into_string().ok())
                        .unwrap_or_else(|| "a past dealing".to_owned()),
                });
            }
            "order" => {
                let scope = match map.get("scope").and_then(|v| v.clone().into_string().ok()) {
                    Some(text) if text == "target-province" => OrderScope::TargetProvince,
                    Some(text) if text == "all-held" => OrderScope::AllHeld,
                    _ => {
                        return Err(EffectParseError::BadField {
                            index,
                            kind: kind.clone(),
                            field: "scope".to_owned(),
                            expected: "\"target-province\" or \"all-held\"".to_owned(),
                        });
                    }
                };
                let amount = map
                    .get("amount")
                    .and_then(|v| v.as_int().ok())
                    .ok_or_else(|| EffectParseError::BadField {
                        index,
                        kind: kind.clone(),
                        field: "amount".to_owned(),
                        expected: "integer".to_owned(),
                    })?;
                effects.push(ScriptEffect::Order {
                    scope,
                    amount: amount as i32,
                });
            }
            other => {
                return Err(EffectParseError::UnknownKind {
                    index,
                    kind: other.to_owned(),
                });
            }
        }
    }
    Ok(effects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dynamic_from(script: &str) -> Dynamic {
        rhai::Engine::new().eval(script).unwrap()
    }

    #[test]
    fn parses_log_effects() {
        let value = dynamic_from(r#"[#{ kind: "log", message_key: "event.a.quiet-day" }]"#);
        assert_eq!(
            parse_effects(value).unwrap(),
            vec![ScriptEffect::Log {
                message_key: "event.a.quiet-day".to_owned()
            }]
        );
    }

    #[test]
    fn unit_return_means_no_effects() {
        let value = dynamic_from("()");
        assert_eq!(parse_effects(value).unwrap(), Vec::new());
    }

    #[test]
    fn mistyped_roles_are_loud_errors() {
        // A typo'd role used to resolve to nobody and silently no-op;
        // now it fails the parse with the vocabulary spelled out.
        let value = dynamic_from(
            r#"[#{ kind: "opinion", from: "targt-head", toward: "leader",
                   amount: 5, reason: "slight" }]"#,
        );
        assert!(matches!(
            parse_effects(value),
            Err(EffectParseError::BadField { field, .. }) if field == "from"
        ));
    }

    #[test]
    fn rejects_unknown_kinds_and_bad_fields() {
        let unknown = dynamic_from(r#"[#{ kind: "explode" }]"#);
        assert!(matches!(
            parse_effects(unknown),
            Err(EffectParseError::UnknownKind { .. })
        ));

        let missing = dynamic_from(r#"[#{ kind: "log" }]"#);
        assert!(matches!(
            parse_effects(missing),
            Err(EffectParseError::BadField { .. })
        ));

        let not_map = dynamic_from(r#"[42]"#);
        assert!(matches!(
            parse_effects(not_map),
            Err(EffectParseError::NotAMap { .. })
        ));
    }
}
