use std::collections::BTreeMap;

use rbx_dom_weak::types::{Attributes, Variant};

use super::{FrontmatterError, PREFIX};

/// Render properties as a frontmatter directive line (including its trailing newline), or an
/// empty string when there are no properties to record.
///
/// # Errors
///
/// Returns an error when a key cannot be written as a bare name or a value has no frontmatter
/// form, so the caller never writes a directive that would not parse back to the same properties.
pub fn render(properties: &BTreeMap<String, Variant>) -> Result<String, FrontmatterError> {
    if properties.is_empty() {
        return Ok(String::new());
    }
    let entries = properties
        .iter()
        .map(|(key, value)| render_entry(key, value))
        .collect::<Result<Vec<String>, FrontmatterError>>()?;
    Ok(format!("{PREFIX} {{ {} }}\n", entries.join(", ")))
}

fn render_entry(key: &str, value: &Variant) -> Result<String, FrontmatterError> {
    if !is_bare_key(key) {
        return Err(FrontmatterError::BadKey(key.to_owned()));
    }
    Ok(format!("{key} = {}", render_value(value)?))
}

fn is_bare_key(key: &str) -> bool {
    !key.is_empty()
        && key == key.trim()
        && !key.contains(['=', ',', '{', '}', '(', ')', '"', '\n', '\r'])
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "rbx_dom_weak adds variants over time; every unlisted one must become an error, never Debug text"
)]
fn render_value(value: &Variant) -> Result<String, FrontmatterError> {
    let rendered = match value {
        Variant::Bool(flag) => flag.to_string(),
        Variant::Int64(number) => number.to_string(),
        Variant::Float64(float) => format!("{float:?}"),
        Variant::String(text) => format!("\"{}\"", escape(text)),
        Variant::Color3(color) => format!("Color3({:?}, {:?}, {:?})", color.r, color.g, color.b),
        Variant::Vector3(vector) => {
            format!("Vector3({:?}, {:?}, {:?})", vector.x, vector.y, vector.z)
        }
        Variant::Enum(enum_value) => format!("Enum({})", enum_value.to_u32()),
        Variant::Tags(tags) => {
            let items: Vec<String> = tags
                .iter()
                .map(|tag| format!("\"{}\"", escape(tag)))
                .collect();
            format!("Tags({})", items.join(", "))
        }
        Variant::Attributes(attributes) => render_attributes(attributes)?,
        other => return Err(FrontmatterError::Unsupported(format!("{:?}", other.ty()))),
    };
    Ok(rendered)
}

fn render_attributes(attributes: &Attributes) -> Result<String, FrontmatterError> {
    let entries = attributes
        .iter()
        .map(|(key, value)| render_entry(key, value))
        .collect::<Result<Vec<String>, FrontmatterError>>()?;
    Ok(format!("Attributes({{ {} }})", entries.join(", ")))
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
