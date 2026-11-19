// SPDX-License-Identifier: GPL-3.0-only
//! JSON Schemas of the documents, and the check that refuses a document of a version we do not speak.

use std::fmt;

use schemars::schema_for;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{
    BANDS_VERSIONS, BandsDoc, DEFAULTS_VERSIONS, Defaults, EVENTS_VERSIONS, Event,
    MANIFEST_VERSIONS, Manifest, PLAN_VERSIONS, Plan, REQUEST_VERSIONS, RESULT_VERSIONS, Request,
    ResultDoc, VERIFY_VERSIONS, VerifyDoc,
};

/// Which document a schema or check is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// The plan document.
    Plan,
    /// The result document.
    Result,
    /// The request document.
    Request,
    /// The manifest `generate` leaves in its output folder.
    Manifest,
    /// The report of `verify --json`.
    Verify,
    /// One progress event line.
    Events,
    /// The request options with their defaults (`lodwright defaults --json`).
    Defaults,
    /// The minimum and maximum of each statistic over several verify reports (`lodwright bands --json`).
    Bands,
}

impl DocumentKind {
    /// Every kind, in the order the handshake lists them.
    pub const ALL: [Self; 8] = [
        Self::Plan,
        Self::Result,
        Self::Request,
        Self::Manifest,
        Self::Verify,
        Self::Events,
        Self::Defaults,
        Self::Bands,
    ];

    /// `plan`, `result`, `request`, `manifest`, `verify`, `events`, `defaults` or `bands`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Result => "result",
            Self::Request => "request",
            Self::Manifest => "manifest",
            Self::Verify => "verify",
            Self::Events => "events",
            Self::Defaults => "defaults",
            Self::Bands => "bands",
        }
    }

    /// Parse a name that [`DocumentKind::name`] gives.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == text)
    }

    /// The contract name, for example `lodwright-plan`.
    #[must_use]
    pub fn contract(self) -> &'static str {
        match self {
            Self::Plan => crate::PLAN_CONTRACT,
            Self::Result => crate::RESULT_CONTRACT,
            Self::Request => crate::REQUEST_CONTRACT,
            Self::Manifest => crate::MANIFEST_CONTRACT,
            Self::Verify => crate::VERIFY_CONTRACT,
            Self::Events => crate::EVENTS_CONTRACT,
            Self::Defaults => crate::DEFAULTS_CONTRACT,
            Self::Bands => crate::BANDS_CONTRACT,
        }
    }

    /// The versions of this document that this build speaks.
    #[must_use]
    pub fn versions(self) -> &'static [u32] {
        match self {
            Self::Plan => PLAN_VERSIONS,
            Self::Result => RESULT_VERSIONS,
            Self::Request => REQUEST_VERSIONS,
            Self::Manifest => MANIFEST_VERSIONS,
            Self::Verify => VERIFY_VERSIONS,
            Self::Events => EVENTS_VERSIONS,
            Self::Defaults => DEFAULTS_VERSIONS,
            Self::Bands => BANDS_VERSIONS,
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
        DocumentKind::Request => schema_for!(Request),
        DocumentKind::Manifest => schema_for!(Manifest),
        DocumentKind::Verify => schema_for!(VerifyDoc),
        DocumentKind::Events => schema_for!(Event),
        DocumentKind::Defaults => schema_for!(Defaults),
        DocumentKind::Bands => schema_for!(BandsDoc),
    };
    let mut text = serde_json::to_string_pretty(&schema).expect("a schema serialises");
    text.push('\n');
    Ok(text)
}

fn check<T: DeserializeOwned>(kind: DocumentKind, value: &Value) -> Result<(), SchemaError> {
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
    let wanted = kind.contract();
    match value.get("contract").and_then(Value::as_str) {
        Some(name) if name == wanted => {}
        Some(other) => {
            return Err(SchemaError::Invalid(format!(
                "the document names the contract {other:?}, not {wanted:?}"
            )));
        }
        None => return Err(SchemaError::Invalid("contract is missing".into())),
    }
    serde_json::from_value::<T>(value.clone())
        .map(drop)
        .map_err(|e| SchemaError::Invalid(e.to_string()))
}

/// Check a plan document: a known `schema_version` and every required field. Unknown fields are ignored.
///
/// # Errors
/// [`SchemaError`] when the document is refused.
pub fn check_plan(value: &Value) -> Result<(), SchemaError> {
    check::<Plan>(DocumentKind::Plan, value)
}

/// Check a request document, as [`check_plan`] does for a plan.
///
/// # Errors
/// [`SchemaError`] when the document is refused.
pub fn check_request(value: &Value) -> Result<(), SchemaError> {
    check::<Request>(DocumentKind::Request, value)
}

/// Check a result document, as [`check_plan`] does for a plan.
///
/// # Errors
/// [`SchemaError`] when the document is refused.
pub fn check_result(value: &Value) -> Result<(), SchemaError> {
    check::<ResultDoc>(DocumentKind::Result, value)
}

/// Check a manifest document, as [`check_plan`] does for a plan.
///
/// # Errors
/// [`SchemaError`] when the document is refused.
pub fn check_manifest(value: &Value) -> Result<(), SchemaError> {
    check::<Manifest>(DocumentKind::Manifest, value)
}

/// Check a verify document, as [`check_plan`] does for a plan.
///
/// # Errors
/// [`SchemaError`] when the document is refused.
pub fn check_verify(value: &Value) -> Result<(), SchemaError> {
    check::<VerifyDoc>(DocumentKind::Verify, value)
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentKind, SchemaError, check_plan, check_result, check_verify, schema_file_name,
        schema_json,
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
        assert_eq!(
            schema_file_name(DocumentKind::Verify, 1),
            "lodwright-verify.v1.json"
        );
    }

    #[test]
    fn every_kind_parses_from_its_own_name() {
        for kind in DocumentKind::ALL {
            assert_eq!(DocumentKind::parse(kind.name()), Some(kind));
            assert_eq!(kind.contract(), format!("lodwright-{}", kind.name()));
        }
        assert_eq!(DocumentKind::parse("nope"), None);
    }

    #[test]
    fn every_kind_has_a_schema_at_each_version_it_speaks() {
        for kind in DocumentKind::ALL {
            for &version in kind.versions() {
                assert!(schema_json(kind, version).is_ok(), "{}", kind.name());
            }
        }
    }

    #[test]
    fn a_verify_report_naming_the_result_contract_is_refused() {
        let doc = json!({"schema_version": 1, "contract": "lodwright-result"});
        assert!(
            check_verify(&doc)
                .unwrap_err()
                .to_string()
                .contains("not \"lodwright-verify\"")
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
