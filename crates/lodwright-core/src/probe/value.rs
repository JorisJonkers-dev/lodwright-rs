// SPDX-License-Identifier: GPL-3.0-only
//! The report tree and its two renderings (JSON and text).

use std::fmt::{self, Write as _};

/// A probe failed: the input is truncated, not the expected format, or too large. The message names the problem and
/// never contains input bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeError(&'static str);

impl ProbeError {
    /// An error with a fixed message.
    #[must_use]
    pub const fn new(message: &'static str) -> Self {
        Self(message)
    }
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for ProbeError {}

/// A short name taken from a file after checking that it is made of safe characters only. The only way to build
/// one is [`Ident::checked`], which refuses anything else, so a report never carries free text from a file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ident(String);

impl Ident {
    /// `bytes` as an identifier when they are 1 to 64 ASCII letters, digits or underscores and the first is a
    /// letter; `None` otherwise.
    #[must_use]
    pub fn checked(bytes: &[u8]) -> Option<Self> {
        let first = *bytes.first()?;
        let ok = bytes.len() <= 64
            && first.is_ascii_alphabetic()
            && bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'_');
        ok.then(|| Self(bytes.iter().map(|b| char::from(*b)).collect()))
    }

    /// `bytes` as a dotted version number: 1 to 15 ASCII digits and dots; `None` otherwise.
    #[must_use]
    pub fn version(bytes: &[u8]) -> Option<Self> {
        let ok = !bytes.is_empty()
            && bytes.len() <= 15
            && bytes.iter().all(|b| b.is_ascii_digit() || *b == b'.');
        ok.then(|| Self(bytes.iter().map(|b| char::from(*b)).collect()))
    }

    /// The identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One node of a report.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A signed integer.
    Int(i64),
    /// An unsigned integer.
    Uint(u64),
    /// A real number, printed with three decimals.
    Float(f64),
    /// A yes or no.
    Bool(bool),
    /// A fixed label of the program (never input text).
    Label(&'static str),
    /// A sanitised name from the input.
    Ident(Ident),
    /// A list.
    List(Vec<Value>),
    /// Named fields in order.
    Map(Vec<(&'static str, Value)>),
}

impl Value {
    /// An unsigned integer from any count.
    #[must_use]
    pub fn count(n: usize) -> Self {
        Self::Uint(u64::try_from(n).unwrap_or(u64::MAX))
    }

    /// The report as one line of JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.json(&mut out);
        out
    }

    fn json(&self, out: &mut String) {
        match self {
            Self::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Uint(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Float(x) => {
                let _ = write!(out, "{x:.3}");
            }
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Label(text) => {
                let _ = write!(out, "\"{text}\"");
            }
            Self::Ident(ident) => {
                let _ = write!(out, "\"{}\"", ident.as_str());
            }
            Self::List(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.json(out);
                }
                out.push(']');
            }
            Self::Map(fields) => {
                out.push('{');
                for (i, (name, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{name}\":");
                    value.json(out);
                }
                out.push('}');
            }
        }
    }

    /// The report as indented `name: value` lines.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        self.text(0, &mut out);
        out
    }

    fn scalar(&self) -> Option<String> {
        match self {
            Self::Int(n) => Some(n.to_string()),
            Self::Uint(n) => Some(n.to_string()),
            Self::Float(x) => Some(format!("{x:.3}")),
            Self::Bool(b) => Some(b.to_string()),
            Self::Label(text) => Some((*text).to_owned()),
            Self::Ident(ident) => Some(ident.as_str().to_owned()),
            Self::List(_) | Self::Map(_) => None,
        }
    }

    fn text(&self, indent: usize, out: &mut String) {
        let pad = "  ".repeat(indent);
        match self {
            Self::Map(fields) => {
                for (name, value) in fields {
                    if let Some(text) = value.scalar() {
                        let _ = writeln!(out, "{pad}{name}: {text}");
                    } else if let Self::List(items) = value {
                        if items.iter().all(|i| i.scalar().is_some()) {
                            let line: Vec<String> = items.iter().filter_map(Self::scalar).collect();
                            let _ = writeln!(out, "{pad}{name}: [{}]", line.join(", "));
                        } else {
                            let _ = writeln!(out, "{pad}{name}:");
                            for item in items {
                                let _ = writeln!(out, "{pad}  -");
                                item.text(indent + 2, out);
                            }
                        }
                    } else {
                        let _ = writeln!(out, "{pad}{name}:");
                        value.text(indent + 1, out);
                    }
                }
            }
            other => {
                if let Some(text) = other.scalar() {
                    let _ = writeln!(out, "{pad}{text}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Ident, ProbeError, Value};

    fn sample() -> Value {
        Value::Map(vec![
            ("size", Value::Uint(12)),
            ("signed", Value::Int(-3)),
            ("mean", Value::Float(1.5)),
            ("ok", Value::Bool(true)),
            ("kind", Value::Label("list")),
            ("name", Value::Ident(Ident::checked(b"NiNode").unwrap())),
            ("ints", Value::List(vec![Value::Uint(1), Value::Uint(2)])),
            (
                "items",
                Value::List(vec![Value::Map(vec![("a", Value::Uint(1))])]),
            ),
            ("nested", Value::Map(vec![("b", Value::Uint(2))])),
        ])
    }

    #[test]
    fn json_is_one_line_with_fields_in_order() {
        assert_eq!(
            sample().to_json(),
            r#"{"size":12,"signed":-3,"mean":1.500,"ok":true,"kind":"list","name":"NiNode","ints":[1,2],"items":[{"a":1}],"nested":{"b":2}}"#
        );
    }

    #[test]
    fn text_nests_maps_and_lists() {
        assert_eq!(
            sample().to_text(),
            "size: 12\nsigned: -3\nmean: 1.500\nok: true\nkind: list\nname: NiNode\nints: [1, 2]\nitems:\n  -\n    a: 1\nnested:\n  b: 2\n"
        );
    }

    #[test]
    fn a_bare_scalar_renders_as_a_line() {
        assert_eq!(Value::Uint(5).to_text(), "5\n");
    }

    #[test]
    fn identifiers_refuse_free_text() {
        assert!(Ident::checked(b"NiTriShapeData").is_some());
        for bad in [
            &b""[..],
            b"1abc",
            b"has space",
            b"quote\"",
            b"new\nline",
            &[b'a'; 65][..],
            "caf\u{e9}".as_bytes(),
        ] {
            assert!(Ident::checked(bad).is_none(), "{bad:?}");
        }
        assert!(Ident::checked(&[b'a'; 64]).is_some());
    }

    #[test]
    fn versions_are_digits_and_dots_only() {
        assert_eq!(Ident::version(b"20.2.0.7").unwrap().as_str(), "20.2.0.7");
        for bad in [&b""[..], b"1.2a", b"1 2", &[b'1'; 16][..]] {
            assert!(Ident::version(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn counts_saturate_and_errors_print_their_message() {
        assert_eq!(Value::count(3), Value::Uint(3));
        assert_eq!(ProbeError::new("boom").to_string(), "boom");
    }
}
