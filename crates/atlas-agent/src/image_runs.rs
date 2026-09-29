//! How an image generator lines up its Image and Style pictures, and the
//! prompt it sends when the person did not write one.
//!
//! Grasshopper stores Flatten and Graft on the input, applied before the
//! component runs. They are mutually exclusive. A flat list is one branch.
//! Flatten keeps that: every picture on the port is one list. Graft puts each
//! picture on its own branch. Branches then match by longest list: the shorter
//! side repeats its last branch. One matched pair is one generation.
//!
//! One base and three styles, style flattened: one run that sees all three
//! styles. Style grafted: three runs, the base repeated. Grafting the base
//! instead batches by base image. Grafting both zips them and repeats the
//! last picture of the shorter list.
//!
//! The stand-in prompt is written for the model that will read it. GPT Image
//! 2.5 and later follow one sentence and get worse when the instruction is a
//! lecture. Earlier GPT Image models copy the style picture's subject unless
//! the roles are explicit. A Codex agent will ask or describe unless the
//! instruction is an imperative. A local diffusion model never sees image
//! numbers: its text encoder should not be told about "image 1", because the
//! style picture arrives as a separate conditioning input.

use crate::{ContextItem, InputSlot, InputSnapshot};

/// Grasshopper's data mapping on one input. `None` and `Flatten` are the same
/// for a flat list of wires; the menu still records which the person chose.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum DataMapping {
    #[default]
    None,
    Flatten,
    Graft,
}

/// Mapping stored on an image generator, for the two picture inputs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct InputMapping {
    #[serde(default)]
    pub media: DataMapping,
    #[serde(default)]
    pub style: DataMapping,
}

impl InputMapping {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn of(self, slot: InputSlot) -> DataMapping {
        match slot {
            InputSlot::Media => self.media,
            InputSlot::Style => self.style,
            _ => DataMapping::None,
        }
    }

    pub fn set(&mut self, slot: InputSlot, mapping: DataMapping) {
        match slot {
            InputSlot::Media => self.media = mapping,
            InputSlot::Style => self.style = mapping,
            _ => {}
        }
    }
}

/// How much role language a model needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// GPT Image 2.5 and later.
    Brief,
    /// GPT Image 2 and earlier, including 1 mini.
    Guided,
    /// A tool-using agent (Codex) that must be told to generate, not discuss.
    Agent,
    /// Local diffusion. The prompt is CLIP text, not a description of attachments.
    Local,
}

/// `gpt-image-2.5-sunburst` is brief. `gpt-image-2` and `gpt-image-1-mini` are guided.
pub fn voice_for_model(model: &str) -> Voice {
    image_version(model)
        .filter(|version| *version >= 2.5)
        .map(|_| Voice::Brief)
        .unwrap_or(Voice::Guided)
}

pub fn voice(provider: &str, model: Option<&str>) -> Voice {
    match provider {
        "comfy" => Voice::Local,
        "codex-image" => Voice::Agent,
        "openai-image" => voice_for_model(model.unwrap_or("gpt-image-2")),
        _ => Voice::Guided,
    }
}

fn image_version(model: &str) -> Option<f32> {
    let rest = model
        .strip_prefix("gpt-image-")
        .or_else(|| model.strip_prefix("chatgpt-image-"))?;
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if num.is_empty() || num == "." {
        return None;
    }
    num.parse().ok()
}

/// One generation per matched branch. Always at least one snapshot.
pub fn split(inputs: &InputSnapshot, mapping: InputMapping) -> Vec<InputSnapshot> {
    let media = branches(leaves(inputs.on(InputSlot::Media)), mapping.media);
    let style = branches(leaves(inputs.on(InputSlot::Style)), mapping.style);
    let rest: Vec<ContextItem> = inputs
        .wired
        .iter()
        .filter(|item| {
            let slot = item.port();
            slot != InputSlot::Media && slot != InputSlot::Style
        })
        .cloned()
        .collect();
    let count = match (media.len(), style.len()) {
        (0, 0) => 1,
        (0, styles) => styles,
        (sources, 0) => sources,
        (sources, styles) => sources.max(styles),
    };
    (0..count)
        .map(|index| {
            let mut wired = rest.clone();
            if !media.is_empty() {
                wired.extend(media[index.min(media.len() - 1)].iter().cloned());
            }
            if !style.is_empty() {
                wired.extend(style[index.min(style.len() - 1)].iter().cloned());
            }
            InputSnapshot {
                revision: format!("{}#{index}", inputs.revision),
                context: inputs.context.clone(),
                wired,
            }
        })
        .collect()
}

fn leaves<'a>(items: impl Iterator<Item = &'a ContextItem>) -> Vec<ContextItem> {
    let mut out = Vec::new();
    for item in items {
        let pictures: Vec<&String> = item.images.iter().filter(|path| !path.is_empty()).collect();
        if item.depth.is_some() || pictures.len() <= 1 {
            out.push(item.clone());
            continue;
        }
        for image in pictures {
            let mut one = item.clone();
            one.images = vec![image.clone()];
            one.depth = None;
            out.push(one);
        }
    }
    out
}

fn branches(leaves: Vec<ContextItem>, mapping: DataMapping) -> Vec<Vec<ContextItem>> {
    if leaves.is_empty() {
        return Vec::new();
    }
    match mapping {
        DataMapping::Graft => leaves.into_iter().map(|leaf| vec![leaf]).collect(),
        DataMapping::None | DataMapping::Flatten => vec![leaves],
    }
}

/// The text the model reads. An empty person-prompt becomes a stand-in only
/// when a style picture is attached. Image numbers follow the order the
/// adapters send: every Image-port picture, then every Style-port picture.
pub fn compose(user: &str, inputs: &InputSnapshot, voice: Voice) -> String {
    let user = user.trim();
    let media = count(inputs, InputSlot::Media);
    let style = count(inputs, InputSlot::Style);
    if user.is_empty() {
        return if style == 0 {
            String::new()
        } else {
            stand_in(media, style, voice)
        };
    }
    match voice {
        Voice::Local => user.to_string(),
        Voice::Brief => match (media > 0, style > 0) {
            (true, true) => format!(
                "{user}\n\n{} is the subject to keep. {} is style only.",
                phrase(1, media),
                phrase(media + 1, style)
            ),
            (true, false) => format!("{user}\n\nTransform the attached image. Keep its composition."),
            (false, true) => format!("{user}\n\nThe attached image is style only."),
            (false, false) => user.to_string(),
        },
        Voice::Guided | Voice::Agent => match (media > 0, style > 0) {
            (true, true) if media == 1 && style == 1 => format!("{user}\n\nThe first image is the source to transform. Keep its composition. The second image is a style reference only: follow its look, not its subject."),
            (true, true) => format!(
                "{user}\n\n{} is the source to transform. Keep its composition. {} is a style reference only: follow its look, not its subject.",
                phrase(1, media),
                phrase(media + 1, style)
            ),
            (true, false) => format!("{user}\n\nTransform the attached image. Keep its composition."),
            (false, true) => format!("{user}\n\nThe attached image is a style reference only: follow its look, not its subject."),
            (false, false) => user.to_string(),
        },
    }
}

fn count(inputs: &InputSnapshot, slot: InputSlot) -> usize {
    inputs
        .on(slot)
        .flat_map(|item| item.images.iter())
        .filter(|path| !path.is_empty())
        .count()
}

fn phrase(start: usize, count: usize) -> String {
    if count <= 1 {
        format!("Image {start}")
    } else if count == 2 {
        format!("Images {start} and {}", start + 1)
    } else {
        format!("Images {start} through {}", start + count - 1)
    }
}

fn stand_in(media: usize, style: usize, voice: Voice) -> String {
    match voice {
        Voice::Local => {
            if media > 0 {
                "the same subject, high quality".into()
            } else {
                "high quality, detailed".into()
            }
        }
        Voice::Brief => {
            if media == 0 {
                if style == 1 {
                    "Create an image in the style of the reference image.".into()
                } else {
                    "Create an image in the combined style of the reference images.".into()
                }
            } else {
                format!(
                    "Apply the style of {} to {}.",
                    phrase(media + 1, style).to_lowercase_first(),
                    phrase(1, media).to_lowercase_first()
                )
            }
        }
        Voice::Guided | Voice::Agent => {
            if media == 0 {
                if style == 1 {
                    "Create an image in the visual style of the reference image: its color, texture, lighting, and medium. Do not copy its subject.".into()
                } else {
                    "Create an image in the combined visual style of the reference images: their color, texture, lighting, and medium. Do not copy their subjects.".into()
                }
            } else {
                format!(
                    "Take the style from {} and apply it to {}. Keep the subject and composition of {}. Follow only the color, texture, lighting, and medium of {}, not the subject.",
                    phrase(media + 1, style).to_lowercase_first(),
                    phrase(1, media).to_lowercase_first(),
                    phrase(1, media).to_lowercase_first(),
                    phrase(media + 1, style).to_lowercase_first()
                )
            }
        }
    }
}

trait LowerFirst {
    fn to_lowercase_first(self) -> String;
}

impl LowerFirst for String {
    fn to_lowercase_first(self) -> String {
        let mut chars = self.chars();
        match chars.next() {
            Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(wired: Vec<ContextItem>) -> InputSnapshot {
        InputSnapshot {
            revision: "rev".into(),
            context: vec![],
            wired,
        }
    }

    fn pictures(node: u64, slot: InputSlot, images: &[&str]) -> ContextItem {
        ContextItem {
            node,
            text: String::new(),
            images: images.iter().map(|path| (*path).to_string()).collect(),
            outputs: Default::default(),
            active: None,
            depth: None,
            slot: Some(slot),
        }
    }

    fn paths(batch: &InputSnapshot, slot: InputSlot) -> Vec<String> {
        batch
            .on(slot)
            .flat_map(|item| item.images.clone())
            .collect()
    }

    #[test]
    fn flatten_uses_every_style_and_graft_makes_one_run_each() {
        let inputs = shot(vec![
            pictures(1, InputSlot::Media, &["base.png"]),
            pictures(2, InputSlot::Style, &["a.jpg"]),
            pictures(3, InputSlot::Style, &["b.jpg"]),
            pictures(4, InputSlot::Style, &["c.jpg"]),
        ]);
        let flat = split(
            &inputs,
            InputMapping {
                style: DataMapping::Flatten,
                ..InputMapping::default()
            },
        );
        assert_eq!(flat.len(), 1);
        assert_eq!(
            paths(&flat[0], InputSlot::Style),
            ["a.jpg", "b.jpg", "c.jpg"]
        );

        let grafted = split(
            &inputs,
            InputMapping {
                style: DataMapping::Graft,
                ..InputMapping::default()
            },
        );
        assert_eq!(grafted.len(), 3);
        for (batch, style) in grafted.iter().zip(["a.jpg", "b.jpg", "c.jpg"]) {
            assert_eq!(paths(batch, InputSlot::Media), ["base.png"]);
            assert_eq!(paths(batch, InputSlot::Style), [style]);
        }
    }

    #[test]
    fn grafting_either_side_batches_that_side_and_both_grafted_zip() {
        let inputs = shot(vec![
            pictures(1, InputSlot::Media, &["b1.png", "b2.png", "b3.png"]),
            pictures(2, InputSlot::Style, &["s.jpg"]),
        ]);
        let by_base = split(
            &inputs,
            InputMapping {
                media: DataMapping::Graft,
                style: DataMapping::Flatten,
            },
        );
        assert_eq!(by_base.len(), 3);
        assert_eq!(paths(&by_base[2], InputSlot::Media), ["b3.png"]);
        assert_eq!(paths(&by_base[2], InputSlot::Style), ["s.jpg"]);

        let zipped = split(
            &inputs,
            InputMapping {
                media: DataMapping::Graft,
                style: DataMapping::Graft,
            },
        );
        // One style branch is repeated onto the longer grafted base list.
        assert_eq!(zipped.len(), 3);
        assert_eq!(paths(&zipped[0], InputSlot::Style), ["s.jpg"]);
        assert_eq!(paths(&zipped[2], InputSlot::Style), ["s.jpg"]);

        let uneven = shot(vec![
            pictures(1, InputSlot::Media, &["b1.png", "b2.png"]),
            pictures(2, InputSlot::Style, &["s1.jpg", "s2.jpg", "s3.jpg"]),
        ]);
        let paired = split(
            &uneven,
            InputMapping {
                media: DataMapping::Graft,
                style: DataMapping::Graft,
            },
        );
        assert_eq!(paired.len(), 3);
        assert_eq!(paths(&paired[1], InputSlot::Media), ["b2.png"]);
        assert_eq!(paths(&paired[2], InputSlot::Media), ["b2.png"]);
        assert_eq!(paths(&paired[2], InputSlot::Style), ["s3.jpg"]);
    }

    #[test]
    fn an_empty_prompt_is_a_short_style_instruction_for_newer_models() {
        let inputs = shot(vec![
            pictures(1, InputSlot::Media, &["base.png"]),
            pictures(2, InputSlot::Style, &["look.jpg"]),
        ]);
        let brief = compose("", &inputs, Voice::Brief);
        let guided = compose("", &inputs, Voice::Guided);
        let local = compose("", &inputs, Voice::Local);
        assert_eq!(brief, "Apply the style of image 2 to image 1.");
        assert!(guided.len() > brief.len());
        assert!(guided.contains("not the subject"));
        assert!(!local.contains("image"));
        assert_eq!(local, "the same subject, high quality");
        assert!(compose("a timber hall", &inputs, Voice::Brief).starts_with("a timber hall"));
        assert_eq!(
            compose("a timber hall", &inputs, Voice::Local),
            "a timber hall"
        );
        assert_eq!(voice_for_model("gpt-image-2.5-sunburst"), Voice::Brief);
        assert_eq!(voice_for_model("gpt-image-2"), Voice::Guided);
        assert_eq!(voice_for_model("gpt-image-1-mini"), Voice::Guided);
        assert_eq!(voice("comfy", None), Voice::Local);
        assert_eq!(voice("codex-image", None), Voice::Agent);
    }

    #[test]
    fn a_written_prompt_still_names_roles_for_guided_models() {
        let inputs = shot(vec![
            pictures(1, InputSlot::Media, &["hall.png"]),
            pictures(2, InputSlot::Style, &["monet.jpg"]),
        ]);
        let text = compose("a watercolor pavilion", &inputs, Voice::Guided);
        assert!(text.contains("style reference only"));
        assert!(text.starts_with("a watercolor pavilion"));
    }
}
