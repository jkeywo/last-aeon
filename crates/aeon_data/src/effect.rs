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
        /// The message text.
        message: String,
    },
    /// Add a directional opinion modifier between job-context roles.
    ///
    /// Roles are resolved by the simulation from the job's context:
    /// `leader`, `target`, `target-head`, `owner-head`, `liege-head`,
    /// `consul`, or `sanctora` (every living Sanctora member).
    Opinion {
        /// Role whose opinion changes.
        from: String,
        /// Role the opinion is about.
        toward: String,
        /// Signed amount.
        amount: i32,
        /// Days until expiry; `None` is permanent.
        days: Option<i64>,
        /// Stable reason tag.
        reason: String,
    },
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
                let message = map
                    .get("message")
                    .and_then(|m| m.clone().into_string().ok())
                    .ok_or_else(|| EffectParseError::BadField {
                        index,
                        kind: kind.clone(),
                        field: "message".to_owned(),
                        expected: "string".to_owned(),
                    })?;
                effects.push(ScriptEffect::Log { message });
            }
            "opinion" => {
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
                    from: get_str("from")?,
                    toward: get_str("toward")?,
                    amount: amount as i32,
                    days,
                    reason: get_str("reason")?,
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
        let value = dynamic_from(r#"[#{ kind: "log", message: "A quiet day" }]"#);
        assert_eq!(
            parse_effects(value).unwrap(),
            vec![ScriptEffect::Log {
                message: "A quiet day".to_owned()
            }]
        );
    }

    #[test]
    fn unit_return_means_no_effects() {
        let value = dynamic_from("()");
        assert_eq!(parse_effects(value).unwrap(), Vec::new());
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
