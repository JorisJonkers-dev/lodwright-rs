// SPDX-License-Identifier: MPL-2.0
//! JSON Schemas of the documents, and the check that refuses a document of a version we do not speak.

use std::fmt;

use schemars::schema_for;
use serde_json::Value;

use crate::{PLAN_VERSIONS, Plan, RESULT_VERSIONS, ResultDoc};

/// Which document a schema or check is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// The plan document.
    Plan,
    /// The result document.
    Result,
}

impl DocumentKind {
    /// `plan` or `result`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Result => "result",
        }
    }

    /// Parse `plan` or `result`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "plan" => Some(Self::Plan),
            "result" => Some(Self::Result),
            _ => None,
        }
    }

    /// The versions of this document that this build speaks.
    #[must_use]
    pub fn versions(self) -> &'static [u32] {
        match self {
            Self::Plan => PLAN_VERSIONS,
            Self::Result => RESULT_VERSIONS,
        }
    }
}

/// Why a document or a requested version was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// The version is not one this build speaks.
    UnsupportedVersion {
        /// The document.
        kind: DocumentKind,
        /// What was asked for or found.
        found: String,
    },
    /// The document does not match the schema of its version.
    Invalid(String),
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { kind, found } => write!(
                f,
                "{} schema_version {found} is not supported; this build speaks {:?}",
                kind.name(),
                kind.versions()
            ),
            Self::Invalid(why) => write!(f, "the document does not match its schema: {why}"),
        }
    }
}

impl std::error::Error for SchemaError {}

/// The file name a release publishes the schema under, for example `lodwright-plan.v1.json`.
#[must_use]
pub fn schema_file_name(kind: DocumentKind, version: u32) -> String {
    format!("lodwright-{}.v{version}.json", kind.name())
}

/// The JSON Schema of `kind` at `version`, pretty-printed with a final newline.
///
/// # Errors
/// [`SchemaError::UnsupportedVersion`] when this build does not speak `version`.
///
/// # Panics
/// Never: a schema is JSON.
pub fn schema_json(kind: DocumentKind, version: u32) -> Result<String, SchemaError> {
    if !kind.versions().contains(&version) {
        return Err(SchemaError::UnsupportedVersion {
            kind,
            found: version.to_string(),
        });
    }
    let schema = match kind {
        DocumentKind::Plan => schema_for!(Plan),
        DocumentKind::Result => schema_for!(ResultDoc),
    };
    let mut text = serde_json::to_string_pretty(&schema).expect("a schema serialises");
    text.push('\n');
    Ok(text)
}

fn check(kind: DocumentKind, value: &Value) -> Result<(), SchemaError> {
    match value.get("schema_version") {
        None => return Err(SchemaError::Invalid("schema_version is missing".into())),
        Some(found) => {
            let known = found
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .is_some_and(|n| kind.versions().contains(&n));
            if !known {
                return Err(SchemaError::UnsupportedVersion {
                    kind,
                    found: found.to_string(),
                });
            }
        }
    }
    let wanted = match kind {
        DocumentKind::Plan => crate::PLAN_CONTRACT,
        DocumentKind::Result => crate::RESULT_CONTRACT,
    };
    match value.get("contract").and_then(Value::as_str) {
        Some(name) if name == wanted => {}
        Some(other) => {
            return Err(SchemaError::Invalid(format!(
                "the document names the contract {other:?}, not {wanted:?}"
            )));
        }
        None => return Err(SchemaError::Invalid("contract is missing".into())),
    }
    let outcome = match kind {
        DocumentKind::Plan => serde_json::from_value::<Plan>(value.clone()).map(drop),
        DocumentKind::Result => serde_json::from_value::<ResultDoc>(value.clone()).map(drop),
    };
    outcome.map_err(|e| SchemaError::Invalid(e.to_string()))
}

/// Check a plan document: a known `schema_version` and every required field. Unknown fields are ignored.
///
/// # Errors
/// [`SchemaError`] when the document is refused.
pub fn check_plan(value: &Value) -> Result<(), SchemaError> {
    check(DocumentKind::Plan, value)
}

/// Check a result document, as [`check_plan`] does for a plan.
///
/// # Errors
/// [`SchemaError`] when the document is refused.
pub fn check_result(value: &Value) -> Result<(), SchemaError> {
    check(DocumentKind::Result, value)
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentKind, SchemaError, check_plan, check_result, schema_file_name, schema_json,
    };
    use serde_json::json;

    #[test]
    fn schema_names_carry_the_kind_and_version() {
        assert_eq!(
            schema_file_name(DocumentKind::Plan, 1),
            "lodwright-plan.v1.json"
        );
        assert_eq!(
            schema_file_name(DocumentKind::Result, 1),
            "lodwright-result.v1.json"
        );
    }

    #[test]
    fn an_unsupported_schema_version_is_refused() {
        let error = schema_json(DocumentKind::Plan, 2).unwrap_err();
        assert!(matches!(error, SchemaError::UnsupportedVersion { .. }));
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn the_schema_pins_the_schema_version_to_one() {
        let text = schema_json(DocumentKind::Plan, 1).unwrap();
        let schema: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(schema["properties"]["schema_version"]["const"], 1);
        assert!(text.ends_with("}\n"));
    }

    #[test]
    fn a_plan_with_an_unknown_schema_version_is_refused() {
        for version in [json!(2), json!(0), json!("1"), json!(-1)] {
            let doc = json!({"schema_version": version});
            assert!(matches!(
                check_plan(&doc),
                Err(SchemaError::UnsupportedVersion { .. })
            ));
        }
        assert!(matches!(
            check_plan(&json!({})),
            Err(SchemaError::Invalid(_))
        ));
    }

    #[test]
    fn a_document_naming_another_contract_is_refused() {
        let doc = json!({"schema_version": 1, "contract": "lodwright-result"});
        assert!(
            check_plan(&doc)
                .unwrap_err()
                .to_string()
                .contains("not \"lodwright-plan\"")
        );
        let doc = json!({"schema_version": 1});
        assert!(
            check_plan(&doc)
                .unwrap_err()
                .to_string()
                .contains("contract is missing")
        );
    }

    #[test]
    fn a_plan_missing_a_required_field_is_refused() {
        let doc = json!({"schema_version": 1, "worlds": []});
        assert!(matches!(check_plan(&doc), Err(SchemaError::Invalid(_))));
    }

    #[test]
    fn a_result_with_extra_fields_is_accepted() {
        let doc = json!({
            "schema_version": 1,
            "contract": "lodwright-result",
            "tool": {"name": "lodwright", "version": "9.9.9", "build": "x"},
            "status": "unsupported", "code": "c", "reason": "r", "future": [1, 2]
        });
        assert_eq!(check_result(&doc), Ok(()));
    }
}
