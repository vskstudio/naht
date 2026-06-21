//! Inline property frontmatter: `--!naht { Disabled = true }`.
//!
//! Naht keeps non-default properties with the source as a leading directive comment instead of
//! Rojo's separate `.meta.json` files. This module parses that line off a script body and renders
//! it back, so the body and its properties round-trip exactly.

use std::collections::BTreeMap;

use rbx_dom_weak::types::{Attributes, Color3, Enum, Tags, Variant, Vector3};

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

/// Split on `delim`, but ignore delimiters inside double-quoted spans or nested `(...)`/`{...}`
/// groups (so a `Color3(1, 0, 0)` or `Attributes({ A = 1 })` value survives intact).
fn split_top_level(s: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut depth = 0u32;
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
            '(' | '{' if !in_quotes => {
                depth += 1;
                current.push(c);
            }
            ')' | '}' if !in_quotes => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            c if c == delim && !in_quotes && depth == 0 => {
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
    if let Some(args) = call_args(s, "Color3") {
        let [r, g, b] = floats(s, args)?;
        return Ok(Variant::Color3(Color3::new(r, g, b)));
    }
    if let Some(args) = call_args(s, "Vector3") {
        let [x, y, z] = floats(s, args)?;
        return Ok(Variant::Vector3(Vector3::new(x, y, z)));
    }
    if let Some(args) = call_args(s, "Enum") {
        let value = args
            .trim()
            .parse::<u32>()
            .map_err(|_| FrontmatterError::BadValue(s.to_string()))?;
        return Ok(Variant::Enum(Enum::from_u32(value)));
    }
    if let Some(args) = call_args(s, "Tags") {
        return Ok(Variant::Tags(parse_tags(args)?));
    }
    if let Some(args) = call_args(s, "Attributes") {
        let table = parse_table(args.trim())?;
        let mut attributes = Attributes::new();
        for (key, value) in table {
            attributes.insert(key, value);
        }
        return Ok(Variant::Attributes(attributes));
    }
    if let Ok(int) = s.parse::<i64>() {
        return Ok(Variant::Int64(int));
    }
    if let Ok(float) = s.parse::<f64>() {
        return Ok(Variant::Float64(float));
    }
    Err(FrontmatterError::BadValue(s.to_string()))
}

/// The inside of a `Name(...)` call, or `None` when `s` is not that call.
fn call_args<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    s.strip_prefix(name)?.strip_prefix('(')?.strip_suffix(')')
}

/// Parse exactly `N` comma-separated `f32`s from a constructor's arguments.
fn floats<const N: usize>(whole: &str, args: &str) -> Result<[f32; N], FrontmatterError> {
    let parts = split_top_level(args, ',');
    let mut out = [0.0f32; N];
    if parts.len() != N {
        return Err(FrontmatterError::BadValue(whole.to_string()));
    }
    for (slot, part) in out.iter_mut().zip(parts) {
        *slot = part
            .trim()
            .parse::<f32>()
            .map_err(|_| FrontmatterError::BadValue(whole.to_string()))?;
    }
    Ok(out)
}

fn parse_tags(args: &str) -> Result<Tags, FrontmatterError> {
    let mut tags = Vec::new();
    for part in split_top_level(args, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let text = part
            .strip_prefix('"')
            .and_then(|p| p.strip_suffix('"'))
            .ok_or_else(|| FrontmatterError::BadValue(part.to_string()))?;
        tags.push(unescape(text));
    }
    Ok(tags.into())
}

fn render_value(value: &Variant) -> String {
    match value {
        Variant::Bool(b) => b.to_string(),
        Variant::Int64(i) => i.to_string(),
        // `{:?}` keeps integral floats as `1.0` so they re-parse as floats, not ints.
        Variant::Float64(f) => format!("{f:?}"),
        Variant::String(s) => format!("\"{}\"", escape(s)),
        Variant::Color3(c) => format!("Color3({:?}, {:?}, {:?})", c.r, c.g, c.b),
        Variant::Vector3(v) => format!("Vector3({:?}, {:?}, {:?})", v.x, v.y, v.z),
        Variant::Enum(e) => format!("Enum({})", e.to_u32()),
        Variant::Tags(tags) => {
            let items: Vec<String> = tags.iter().map(|t| format!("\"{}\"", escape(t))).collect();
            format!("Tags({})", items.join(", "))
        }
        Variant::Attributes(attributes) => {
            let items: Vec<String> = attributes
                .iter()
                .map(|(key, value)| format!("{key} = {}", render_value(value)))
                .collect();
            format!("Attributes({{ {} }})", items.join(", "))
        }
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

    #[test]
    fn structured_property_types_round_trip() {
        let mut props = BTreeMap::new();
        props.insert(
            "Color".to_string(),
            Variant::Color3(Color3::new(1.0, 0.5, 0.0)),
        );
        props.insert(
            "Position".to_string(),
            Variant::Vector3(Vector3::new(1.0, 2.0, 3.0)),
        );
        props.insert("Material".to_string(), Variant::Enum(Enum::from_u32(256)));
        props.insert(
            "Tags".to_string(),
            Variant::Tags(vec!["combat".to_string(), "npc".to_string()].into()),
        );

        let line = render(&props).unwrap();
        let (parsed, body) = split(&format!("{line}body")).unwrap();
        assert_eq!(parsed, props);
        assert_eq!(body, "body");
    }

    #[test]
    fn attributes_round_trip() {
        let mut attributes = Attributes::new();
        attributes.insert("Health".to_string(), Variant::Float64(100.0));
        attributes.insert("Title".to_string(), Variant::String("hero".to_string()));
        let mut props = BTreeMap::new();
        props.insert("Attributes".to_string(), Variant::Attributes(attributes));

        let line = render(&props).unwrap();
        let (parsed, _) = split(&format!("{line}rest")).unwrap();
        assert_eq!(parsed, props);
    }

    #[test]
    fn multi_line_source_survives_the_split() {
        let mut props = BTreeMap::new();
        props.insert("Disabled".to_string(), Variant::Bool(true));
        let line = render(&props).unwrap();
        let source = "local x = 1\nlocal y = 2\nprint(x + y)\n";

        let (parsed, body) = split(&format!("{line}{source}")).unwrap();
        assert_eq!(parsed, props);
        assert_eq!(body, source);
    }
}
