//! Construct an inert Rhino 8 archive from a provider's text files. The XML
//! fragments are Grasshopper-serialized component layouts. Only the existing
//! reviewed revision path can open or execute the resulting C# script.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub(super) const GUIDE: &str = include_str!("recipe/guide.md");
const MANIFEST: &str = "parameters.json";
const BODY: &str = "geometry.cs";
const INPUT_LIMIT: usize = 32;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    version: u32,
    title: String,
    inputs: Vec<Input>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    key: String,
    name: String,
    #[serde(rename = "type")]
    kind: Kind,
    default: Value,
    min: Option<f64>,
    max: Option<f64>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Number,
    Integer,
    Boolean,
    Text,
}

impl Kind {
    // Rhino deserializes these fields unconditionally before restoring a contextual input.
    fn contextual_settings(self) -> (usize, &'static str) {
        match self {
            Self::Number => (
                14,
                r#"<item name="Minimum" type_name="gh_double" type_code="6">-1.7976931348623157E+308</item><item name="Maximum" type_name="gh_double" type_code="6">1.7976931348623157E+308</item>"#,
            ),
            Self::Integer => (
                14,
                r#"<item name="Minimum" type_name="gh_int32" type_code="3">-2147483648</item><item name="Maximum" type_name="gh_int32" type_code="3">2147483647</item>"#,
            ),
            Self::Boolean => (
                14,
                r#"<item name="TrueValue" type_name="gh_string" type_code="10">True</item><item name="FalseValue" type_name="gh_string" type_code="10">False</item>"#,
            ),
            Self::Text => (12, ""),
        }
    }

    fn getter(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Number => (
                "Get Number",
                "7b36b876-9451-46f5-8220-a200d969cc66",
                "ToDouble",
            ),
            Self::Integer => (
                "Get Integer",
                "b228887e-0852-4d9f-bd46-2591646e0d7c",
                "ToInt32",
            ),
            Self::Boolean => (
                "Get Boolean",
                "51ef601d-f86e-4ee4-bcf2-3d459d3e95e9",
                "ToBoolean",
            ),
            Self::Text => (
                "Get String",
                "fed87bdd-8327-49cd-949c-09d70f3c345c",
                "ToString",
            ),
        }
    }
}

pub(super) fn empty_xml(bytes: &[u8]) -> bool {
    let Ok(xml) = std::str::from_utf8(bytes) else {
        return false;
    };
    let Ok(doc) = roxmltree::Document::parse(xml.trim_start_matches('\u{feff}')) else {
        return false;
    };
    let root = doc.root_element();
    fn chunk<'a, 'input>(
        node: roxmltree::Node<'a, 'input>,
        name: &str,
    ) -> Option<roxmltree::Node<'a, 'input>> {
        node.children()
            .find(|n| n.has_tag_name("chunks"))?
            .children()
            .find(|n| n.has_tag_name("chunk") && n.attribute("name") == Some(name))
    }
    if !root.has_tag_name("Archive") {
        return false;
    }
    let Some(definition) = chunk(root, "Definition") else {
        return false;
    };
    let Some(objects) = chunk(definition, "DefinitionObjects") else {
        return false;
    };
    !objects
        .descendants()
        .any(|n| n.has_tag_name("chunk") && n.attribute("name") == Some("Object"))
        && objects.descendants().any(|n| {
            n.has_tag_name("item")
                && n.attribute("name") == Some("ObjectCount")
                && n.text().is_some_and(|v| v.trim() == "0")
        })
}

pub(super) fn from_stage(draft: &Path) -> Result<Vec<u8>, String> {
    let manifest = super::bounded_bytes_upto(&draft.join(MANIFEST), 64 * 1024)?;
    let body = super::bounded_bytes_upto(&draft.join(BODY), 256 * 1024)?;
    let text = std::str::from_utf8(&body).map_err(|_| "geometry.cs must be UTF-8")?;
    let seed = draft
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Missing request directory")?;
    generate(seed, &manifest, text.trim_start_matches('\u{feff}')).map(String::into_bytes)
}

fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn valid_label(text: &str, max: usize) -> bool {
    !text.trim().is_empty()
        && text.len() <= max
        && text
            .chars()
            .all(|c| !c.is_control() && !matches!(c, '\u{fffe}' | '\u{ffff}'))
}

fn object_id(seed: &str, role: &str, key: &str) -> String {
    let hash = atlas_ai::versions::hash_artifact_input(format!("{seed}/{role}/{key}").as_bytes());
    format!(
        "{}-{}-8{}-a{}-{}",
        &hash[..8],
        &hash[8..12],
        &hash[13..16],
        &hash[17..20],
        &hash[20..32]
    )
}

impl Input {
    fn control(&self, index: usize, id: &str, y: usize) -> Result<String, String> {
        let name = xml_text(&self.name);
        let x = 40;
        match self.kind {
            Kind::Number | Kind::Integer => {
                let value = self.default.as_f64().ok_or("default must be a number")?;
                let min = self.min.ok_or("missing slider min")?;
                let max = self.max.ok_or("missing slider max")?;
                if !(min < max && (min..=max).contains(&value))
                    || [min, max, value].iter().any(|v| v.abs() > 1.0e15)
                {
                    return Err(
                        "slider default must be inside its finite min/max range (up to 1e15)"
                            .into(),
                    );
                }
                let integer = matches!(self.kind, Kind::Integer);
                if integer
                    && [min, max, value]
                        .iter()
                        .any(|v| v.fract() != 0. || *v < i32::MIN as f64 || *v > i32::MAX as f64)
                {
                    return Err(
                        "integer slider bounds and default must fit a 32-bit integer".into(),
                    );
                }
                let interval = usize::from(integer);
                let digits = if integer { 0 } else { 6 };
                Ok(format!(
                    include_str!("recipe/slider.xml"),
                    index = index,
                    id = id,
                    name = name,
                    x = x,
                    y = y,
                    interval = interval,
                    digits = digits,
                    min = min,
                    max = max,
                    value = value
                ))
            }
            Kind::Boolean | Kind::Text if self.min.is_some() || self.max.is_some() => {
                Err("only numeric sliders accept min and max".into())
            }
            Kind::Boolean => {
                let value = self.default.as_bool().ok_or("default must be a boolean")?;
                Ok(format!(
                    include_str!("recipe/toggle.xml"),
                    index = index,
                    id = id,
                    name = name,
                    x = x,
                    y = y,
                    value = value
                ))
            }
            Kind::Text => {
                let value = self.default.as_str().ok_or("default must be text")?;
                if value.len() > 8192
                    || value.chars().any(|c| {
                        (c.is_control() && !matches!(c, '\t' | '\r' | '\n'))
                            || matches!(c, '\u{fffe}' | '\u{ffff}')
                    })
                {
                    return Err(
                        "text default exceeds its length or contains invalid XML characters".into(),
                    );
                }
                let value = xml_text(value);
                Ok(format!(
                    include_str!("recipe/panel.xml"),
                    index = index,
                    id = id,
                    name = name,
                    x = x,
                    y = y,
                    value = value
                ))
            }
        }
    }
}

fn generate(seed: &str, manifest: &[u8], body: &str) -> Result<String, String> {
    let recipe: Recipe = serde_json::from_slice(
        manifest
            .strip_prefix(&[0xef, 0xbb, 0xbf])
            .unwrap_or(manifest),
    )
    .map_err(|e| format!("parameters.json: {e}"))?;
    if recipe.version != 1
        || recipe.inputs.len() > INPUT_LIMIT
        || !valid_label(&recipe.title, 128)
        || body.trim().is_empty()
    {
        return Err(
            "Expected recipe version 1, a title, at most 32 inputs and a nonempty geometry.cs"
                .into(),
        );
    }
    let mut keys = HashSet::new();
    let mut objects = String::new();
    let mut arguments = Vec::new();
    let mut locals = String::new();
    let mut input_ids = String::new();
    let mut input_params = String::new();
    for (i, input) in recipe.inputs.iter().enumerate() {
        let key = input.key.as_str();
        if key.len() > 64
            || !key.starts_with(|c: char| c.is_ascii_alphabetic())
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || key == "model"
            || !keys.insert(key)
            || !valid_label(&input.name, 128)
        {
            return Err(format!(
                "Input {} requires a unique ASCII identifier and readable name",
                i + 1
            ));
        }
        let control = object_id(seed, "control", key);
        let port = object_id(seed, "port", key);
        let y = 75 + i * 145;
        objects.push_str(
            &input
                .control(i * 2, &control, y)
                .map_err(|e| format!("{}: {e}", input.name))?,
        );
        let (kind, class, convert) = input.kind.getter();
        let name = xml_text(&input.name);
        let (items, settings) = input.kind.contextual_settings();
        objects.push_str(&format!(
            include_str!("recipe/getter.xml"),
            index = i * 2 + 1,
            id = port,
            name = name,
            x = 310,
            y = y,
            items = items,
            settings = settings,
            kind = kind,
            class = class,
            source = control
        ));
        // A reserved identifier is valid when escaped with @. The __slot prefix
        // cannot collide with a manifest key (keys must start with a letter).
        let arg = format!("__slot{i}");
        arguments.push(format!("object {arg}"));
        locals.push_str(&format!("var @{key} = System.Convert.{convert}({arg},System.Globalization.CultureInfo.InvariantCulture);\n"));
        input_ids.push_str(&format!(r#"<item name="InputId" index="{i}" type_name="gh_guid" type_code="9">08908df5-fa14-4982-9ab2-1aa0927566aa</item>"#));
        input_params.push_str(&format!(
            include_str!("recipe/script-input.xml"),
            index = i,
            source = port,
            arg = arg,
            id = object_id(seed, "argument", key)
        ));
    }
    let input_count = recipe.inputs.len();
    arguments.push("ref object model".into());
    let args = arguments.join(", ");
    let code = format!(
        "// #! csharp\nusing System;\nusing System.Collections.Generic;\nusing Rhino.Geometry;\n\
         public class Script_Instance : GH_ScriptInstance {{\nprivate void RunScript({args}) {{\n\
         {locals}{body}\n}}\n}}\n"
    );
    let name = xml_text(&recipe.title);
    objects.push_str(&format!(
        include_str!("recipe/script.xml"),
        index = input_count * 2,
        id = object_id(seed, "script", ""),
        name = name,
        x = 570,
        y = 75,
        io_items = input_count + 3,
        input_count = input_count,
        input_ids = input_ids,
        io_chunks = input_count + 1,
        input_params = input_params,
        output_id = object_id(seed, "output", "model"),
        code = STANDARD.encode(code.as_bytes())
    ));
    // GH_IO stores System.DateTime ticks (100 ns periods since January 1, year 1).
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let document_date = 621_355_968_000_000_000 + elapsed.as_nanos() / 100;
    Ok(format!(
        include_str!("recipe/definition.xml"),
        document_date = document_date,
        name = name,
        objects = objects,
        count = input_count * 2 + 1,
        document_id = object_id(seed, "document", "")
    ))
}

#[cfg(test)]
mod tests;
