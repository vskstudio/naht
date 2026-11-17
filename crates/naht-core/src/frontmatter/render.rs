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

fn render_value(value: &Variant) -> Result<String, FrontmatterError> {
    if let Variant::Bool(flag) = value {
        Ok(flag.to_string())
    } else if let Variant::Int64(number) = value {
        Ok(number.to_string())
    } else if let Variant::Float64(float) = value {
        Ok(format!("{float:?}"))
    } else if let Variant::String(text) = value {
        Ok(format!("\"{}\"", escape(text)))
    } else if let Variant::Color3(color) = value {
        Ok(format!(
            "Color3({:?}, {:?}, {:?})",
            color.r, color.g, color.b
        ))
    } else if let Variant::Vector3(vector) = value {
        Ok(format!(
            "Vector3({:?}, {:?}, {:?})",
            vector.x, vector.y, vector.z
        ))
    } else if let Variant::Enum(enum_value) = value {
        Ok(format!("Enum({})", enum_value.to_u32()))
    } else if let Variant::Tags(tags) = value {
        let items: Vec<String> = tags
            .iter()
            .map(|tag| format!("\"{}\"", escape(tag)))
            .collect();
        Ok(format!("Tags({})", items.join(", ")))
    } else if let Variant::Attributes(attributes) = value {
        render_attributes(attributes)
    } else {
        Err(FrontmatterError::Unsupported(format!("{:?}", value.ty())))
    }
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
