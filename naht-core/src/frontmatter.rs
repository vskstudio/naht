//! Inline property frontmatter: `--!naht { Disabled = true }`.
//!
//! Naht keeps non-default properties with the source as a leading directive comment instead of
//! Rojo's separate `.meta.json` files. This module parses that line off a script body and renders
//! it back, so the body and its properties round-trip exactly.

use std::collections::BTreeMap;

use rbx_dom_weak::types::Variant;

/// The directive prefix that marks a frontmatter line.
const PREFIX: &str = "--!naht";

/// Errors from parsing a frontmatter directive.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FrontmatterError {
    /// The directive was not a `{ ... }` table.
    #[error("frontmatter is not a brace-delimited table: {0:?}")]
    NotATable(String),
    /// A `Key = value` pair was malformed.
    #[error("malformed frontmatter entry: {0:?}")]
    MalformedEntry(String),
    /// A value could not be parsed as a bool, number, or quoted string.
    #[error("unparseable frontmatter value: {0:?}")]
    BadValue(String),
}

/// Split a leading frontmatter directive off a script body.
///
/// Returns the parsed properties (empty if there is no directive) and the remaining body with the
/// directive line and its trailing newline removed.
pub fn split(body: &str) -> Result<(BTreeMap<String, Variant>, String), FrontmatterError> {
    let Some(rest) = body.strip_prefix(PREFIX) else {
        return Ok((BTreeMap::new(), body.to_string()));
    };
    // The prefix must be followed by whitespace; otherwise this is some other `--!...` comment
    // (e.g. a hypothetical `--!nahtish`), not our directive, and the body is left untouched.
    if !rest.starts_with(char::is_whitespace) {
        return Ok((BTreeMap::new(), body.to_string()));
    }
    // The directive occupies the first line; the body is whatever follows the newline.
    let (line, remainder) = match rest.split_once('\n') {
        Some((line, remainder)) => (line, remainder.to_string()),
        None => (rest, String::new()),
    };
    let props = parse_table(line.trim())?;
    Ok((props, remainder))
}

/// Render properties as a frontmatter directive line (including its trailing newline), or `None`
/// when there are no properties to record.
#[must_use]
pub fn render(properties: &BTreeMap<String, Variant>) -> Option<String> {
    if properties.is_empty() {
        return None;
    }
    let body = properties
        .iter()
        .map(|(key, value)| format!("{key} = {}", render_value(value)))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("{PREFIX} {{ {body} }}\n"))
}

fn parse_table(s: &str) -> Result<BTreeMap<String, Variant>, FrontmatterError> {
    let inner = s
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| FrontmatterError::NotATable(s.to_string()))?
        .trim();
    let mut props = BTreeMap::new();
    if inner.is_empty() {
        return Ok(props);
    }
    for pair in split_top_level(inner, ',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let mut sides = split_top_level(pair, '=');
        if sides.len() != 2 {
            return Err(FrontmatterError::MalformedEntry(pair.to_string()));
        }
        let value = sides.pop().expect("two sides").trim().to_string();
        let key = sides.pop().expect("two sides").trim().to_string();
        if key.is_empty() {
            return Err(FrontmatterError::MalformedEntry(pair.to_string()));
        }
        props.insert(key, parse_value(&value)?);
    }
    Ok(props)
}

/// Split on `delim`, but ignore delimiters inside double-quoted spans.
fn split_top_level(s: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                current.push(c);
            }
            '\\' if in_quotes => {
                current.push(c);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c == delim && !in_quotes => {
                parts.push(current.clone());
                current.clear();
            }
            c => current.push(c),
        }
    }
    parts.push(current);
    parts
}

fn parse_value(s: &str) -> Result<Variant, FrontmatterError> {
    match s {
        "true" => return Ok(Variant::Bool(true)),
        "false" => return Ok(Variant::Bool(false)),
        _ => {}
    }
    if let Some(text) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Ok(Variant::String(unescape(text)));
    }
    if let Ok(int) = s.parse::<i64>() {
        return Ok(Variant::Int64(int));
    }
    if let Ok(float) = s.parse::<f64>() {
        return Ok(Variant::Float64(float));
    }
    Err(FrontmatterError::BadValue(s.to_string()))
}

fn render_value(value: &Variant) -> String {
    match value {
        Variant::Bool(b) => b.to_string(),
        Variant::Int64(i) => i.to_string(),
        // `{:?}` keeps integral floats as `1.0` so they re-parse as floats, not ints.
        Variant::Float64(f) => format!("{f:?}"),
        Variant::String(s) => format!("\"{}\"", escape(s)),
        other => format!("{other:?}"),
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_directive_leaves_body_untouched() {
        let (props, body) = split("print('hi')\n").unwrap();
        assert!(props.is_empty());
        assert_eq!(body, "print('hi')\n");
    }

    #[test]
    fn parses_bool_and_strips_line() {
        let (props, body) = split("--!naht { Disabled = true }\nprint('hi')\n").unwrap();
        assert_eq!(props.get("Disabled"), Some(&Variant::Bool(true)));
        assert_eq!(body, "print('hi')\n");
    }

    #[test]
    fn other_directive_comments_are_not_frontmatter() {
        // A comment that merely shares the prefix's first bytes is left as source, not an error.
        let (props, body) = split("--!nahtish whatever\nprint('hi')\n").unwrap();
        assert!(props.is_empty());
        assert_eq!(body, "--!nahtish whatever\nprint('hi')\n");
    }

    #[test]
    fn parses_mixed_value_types() {
        let (props, _) =
            split("--!naht { Disabled = false, Order = 3, Name = \"a, b\" }\n").unwrap();
        assert_eq!(props.get("Disabled"), Some(&Variant::Bool(false)));
        assert_eq!(props.get("Order"), Some(&Variant::Int64(3)));
        assert_eq!(
            props.get("Name"),
            Some(&Variant::String("a, b".to_string()))
        );
    }

    #[test]
    fn render_then_split_round_trips() {
        let mut props = BTreeMap::new();
        props.insert("Disabled".to_string(), Variant::Bool(true));
        props.insert("Order".to_string(), Variant::Int64(2));
        props.insert("Ratio".to_string(), Variant::Float64(1.0));
        let line = render(&props).unwrap();
        let (parsed, body) = split(&format!("{line}body")).unwrap();
        assert_eq!(parsed, props);
        assert_eq!(body, "body");
    }

    #[test]
    fn empty_properties_render_to_nothing() {
        assert_eq!(render(&BTreeMap::new()), None);
    }
}
