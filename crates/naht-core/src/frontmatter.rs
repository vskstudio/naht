//! Inline property frontmatter: `--!naht { Disabled = true }`.
//!
//! Naht keeps non-default properties with the source as a leading directive comment instead of
//! Rojo's separate `.meta.json` files. This module parses that line off a script body and renders
//! it back, so the body and its properties round-trip exactly.

use std::collections::BTreeMap;
use std::num::{ParseFloatError, ParseIntError};

use rbx_dom_weak::types::{Attributes, Color3, Enum, Tags, Variant, Vector3};

const PREFIX: &str = "--!naht";
const MAX_DEPTH: usize = 32;

mod render;

pub use render::render;

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
    /// An enum value was not an unsigned integer.
    #[error("frontmatter enum value {raw:?} is not an unsigned integer")]
    BadEnum {
        /// The raw value as written in the directive.
        raw: String,
        /// The integer parse failure.
        #[source]
        source: ParseIntError,
    },
    /// A vector or color component was not a float.
    #[error("frontmatter component {raw:?} is not a float")]
    BadComponent {
        /// The raw component as written in the directive.
        raw: String,
        /// The float parse failure.
        #[source]
        source: ParseFloatError,
    },
    /// Nested tables went deeper than the parser allows.
    #[error("frontmatter nests deeper than {0} levels")]
    TooDeep(usize),
    /// A property or attribute name cannot be written as a frontmatter key.
    #[error("frontmatter key {0:?} cannot be written as a bare name")]
    BadKey(String),
    /// A property value has no frontmatter form.
    #[error("value of type {0} has no frontmatter form")]
    Unsupported(String),
}

/// Split a leading frontmatter directive off a script body.
///
/// Returns the parsed properties (empty if there is no directive) and the remaining body with the
/// directive line and its trailing newline removed.
///
/// # Errors
///
/// Returns an error when the frontmatter block or one of its entries is malformed.
pub fn split(body: &str) -> Result<(BTreeMap<String, Variant>, String), FrontmatterError> {
    let Some(rest) = directive_rest(body) else {
        return Ok((BTreeMap::new(), body.to_owned()));
    };
    let (line, remainder) = split_directive_line(rest);
    let props = parse_table(line.trim(), 0)?;
    Ok((props, remainder))
}

fn directive_rest(body: &str) -> Option<&str> {
    body.strip_prefix(PREFIX)
        .filter(|rest| rest.starts_with(char::is_whitespace))
}

fn split_directive_line(rest: &str) -> (&str, String) {
    match rest.split_once('\n') {
        Some((line, remainder)) => (line, remainder.to_owned()),
        None => (rest, String::new()),
    }
}

fn parse_table(body: &str, depth: usize) -> Result<BTreeMap<String, Variant>, FrontmatterError> {
    if depth > MAX_DEPTH {
        return Err(FrontmatterError::TooDeep(MAX_DEPTH));
    }
    let inner = body
        .strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
        .ok_or_else(|| FrontmatterError::NotATable(body.to_owned()))?
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
        let sides = split_top_level(pair, '=');
        let [key, value] = sides.as_slice() else {
            return Err(FrontmatterError::MalformedEntry(pair.to_owned()));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(FrontmatterError::MalformedEntry(pair.to_owned()));
        }
        props.insert(key.to_owned(), parse_value(value.trim(), depth)?);
    }
    Ok(props)
}

fn split_top_level(input: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut depth = 0u32;
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' => {
                in_quotes = !in_quotes;
                current.push(character);
            }
            '\\' if in_quotes => {
                current.push(character);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '(' | '{' if !in_quotes => {
                depth = depth.saturating_add(1);
                current.push(character);
            }
            ')' | '}' if !in_quotes => {
                depth = depth.saturating_sub(1);
                current.push(character);
            }
            character if character == delim && !in_quotes && depth == 0 => {
                parts.push(current.clone());
                current.clear();
            }
            character => current.push(character),
        }
    }
    parts.push(current);
    parts
}

fn parse_value(raw: &str, depth: usize) -> Result<Variant, FrontmatterError> {
    match raw {
        "true" => return Ok(Variant::Bool(true)),
        "false" => return Ok(Variant::Bool(false)),
        _ => {}
    }
    if let Some(text) = raw
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
    {
        return Ok(Variant::String(unescape(text)));
    }
    if let Some(args) = call_args(raw, "Color3") {
        let [red, green, blue] = floats(raw, args)?;
        return Ok(Variant::Color3(Color3::new(red, green, blue)));
    }
    if let Some(args) = call_args(raw, "Vector3") {
        let [x_component, y_component, z_component] = floats(raw, args)?;
        return Ok(Variant::Vector3(Vector3::new(
            x_component,
            y_component,
            z_component,
        )));
    }
    if let Some(args) = call_args(raw, "Enum") {
        let value = args
            .trim()
            .parse::<u32>()
            .map_err(|source| FrontmatterError::BadEnum {
                raw: raw.to_owned(),
                source,
            })?;
        return Ok(Variant::Enum(Enum::from_u32(value)));
    }
    if let Some(args) = call_args(raw, "Tags") {
        return Ok(Variant::Tags(parse_tags(args)?));
    }
    if let Some(args) = call_args(raw, "Attributes") {
        let table = parse_table(args.trim(), depth.saturating_add(1))?;
        let mut attributes = Attributes::new();
        for (key, value) in table {
            attributes.insert(key, value);
        }
        return Ok(Variant::Attributes(attributes));
    }
    if let Ok(int) = raw.parse::<i64>() {
        return Ok(Variant::Int64(int));
    }
    if let Ok(float) = raw.parse::<f64>() {
        return Ok(Variant::Float64(float));
    }
    Err(FrontmatterError::BadValue(raw.to_owned()))
}

fn call_args<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    source
        .strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')
}

fn floats<const N: usize>(whole: &str, args: &str) -> Result<[f32; N], FrontmatterError> {
    let parts = split_top_level(args, ',');
    let mut out = [0.0f32; N];
    if parts.len() != N {
        return Err(FrontmatterError::BadValue(whole.to_owned()));
    }
    for (slot, part) in out.iter_mut().zip(parts) {
        *slot = part
            .trim()
            .parse::<f32>()
            .map_err(|source| FrontmatterError::BadComponent {
                raw: whole.to_owned(),
                source,
            })?;
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
            .and_then(|candidate| candidate.strip_suffix('"'))
            .ok_or_else(|| FrontmatterError::BadValue(part.to_owned()))?;
        tags.push(unescape(text));
    }
    Ok(tags.into())
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') | None => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
            }
        } else {
            out.push(character);
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
        assert_eq!(props.get("Name"), Some(&Variant::String("a, b".to_owned())));
    }

    #[test]
    fn render_then_split_round_trips() {
        let mut props = BTreeMap::new();
        props.insert("Disabled".to_owned(), Variant::Bool(true));
        props.insert("Order".to_owned(), Variant::Int64(2));
        props.insert("Ratio".to_owned(), Variant::Float64(1.0));
        let line = render(&props).unwrap();
        let (parsed, body) = split(&format!("{line}body")).unwrap();
        assert_eq!(parsed, props);
        assert_eq!(body, "body");
    }

    #[test]
    fn empty_properties_render_to_nothing() {
        assert_eq!(render(&BTreeMap::new()).unwrap(), "");
    }

    #[test]
    fn structured_property_types_round_trip() {
        let mut props = BTreeMap::new();
        props.insert(
            "Color".to_owned(),
            Variant::Color3(Color3::new(1.0, 0.5, 0.0)),
        );
        props.insert(
            "Position".to_owned(),
            Variant::Vector3(Vector3::new(1.0, 2.0, 3.0)),
        );
        props.insert("Material".to_owned(), Variant::Enum(Enum::from_u32(256)));
        props.insert(
            "Tags".to_owned(),
            Variant::Tags(vec!["combat".to_owned(), "npc".to_owned()].into()),
        );

        let line = render(&props).unwrap();
        let (parsed, body) = split(&format!("{line}body")).unwrap();
        assert_eq!(parsed, props);
        assert_eq!(body, "body");
    }

    #[test]
    fn attributes_round_trip() {
        let mut attributes = Attributes::new();
        attributes.insert("Health".to_owned(), Variant::Float64(100.0));
        attributes.insert("Title".to_owned(), Variant::String("hero".to_owned()));
        let mut props = BTreeMap::new();
        props.insert("Attributes".to_owned(), Variant::Attributes(attributes));

        let line = render(&props).unwrap();
        let (parsed, _) = split(&format!("{line}rest")).unwrap();
        assert_eq!(parsed, props);
    }

    #[test]
    fn multi_line_source_survives_the_split() {
        let mut props = BTreeMap::new();
        props.insert("Disabled".to_owned(), Variant::Bool(true));
        let line = render(&props).unwrap();
        let source = "local x = 1\nlocal y = 2\nprint(x + y)\n";

        let (parsed, body) = split(&format!("{line}{source}")).unwrap();
        assert_eq!(parsed, props);
        assert_eq!(body, source);
    }

    #[test]
    fn a_string_with_newlines_quotes_and_backslashes_round_trips() {
        let mut props = BTreeMap::new();
        props.insert(
            "Source".to_owned(),
            Variant::String("line one\nline \"two\"\r\\end".to_owned()),
        );
        let line = render(&props).unwrap();
        let (parsed, body) = split(&format!("{line}body")).unwrap();
        assert_eq!(parsed, props);
        assert_eq!(body, "body");
    }

    #[test]
    fn a_value_without_a_frontmatter_form_is_an_error_not_debug_text() {
        let mut props = BTreeMap::new();
        props.insert("Size".to_owned(), Variant::Float32(1.0));
        assert!(matches!(
            render(&props),
            Err(FrontmatterError::Unsupported(_))
        ));
    }

    fn nested_attributes(levels: usize) -> String {
        let mut value = String::from("1");
        for _ in 0..levels {
            value = format!("Attributes({{ Inner = {value} }})");
        }
        format!("--!naht {{ Nested = {value} }}\nbody\n")
    }

    #[test]
    fn frontmatter_nested_deeper_than_the_limit_is_rejected() {
        assert!(matches!(
            split(&nested_attributes(40)),
            Err(FrontmatterError::TooDeep(_))
        ));
    }

    #[test]
    fn frontmatter_within_the_nesting_limit_still_parses() {
        let (props, body) = split(&nested_attributes(5)).unwrap();
        assert!(props.contains_key("Nested"));
        assert_eq!(body, "body\n");
    }

    #[test]
    fn newlines_and_backslashes_in_strings_round_trip_as_escapes() {
        let source = "first\nsecond\rthird\\fourth \"quoted\"";
        let properties =
            BTreeMap::from([("Source".to_owned(), Variant::String(source.to_owned()))]);
        let rendered = render(&properties).unwrap();

        assert_eq!(
            rendered,
            "--!naht { Source = \"first\\nsecond\\rthird\\\\fourth \\\"quoted\\\"\" }\n"
        );
        let (parsed, body) = split(&format!("{rendered}body\n")).unwrap();
        assert_eq!(parsed, properties);
        assert_eq!(body, "body\n");
    }
}
