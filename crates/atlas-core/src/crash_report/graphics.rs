use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

const CHECKPOINT: &str = "graphics.json";
const CHECKPOINT_LIMIT: u64 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
enum RendererFamily {
    Intel,
    Mesa,
    #[serde(rename = "AMD")]
    Amd,
    Nvidia,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsCheckpoint {
    renderer_family: RendererFamily,
    gl_major: u32,
    gl_minor: u32,
    requested_msaa: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    driver_build: Option<[u16; 4]>,
}

impl GraphicsCheckpoint {
    /// Reduce GL information to a fixed family, small numbers, and an optional
    /// Intel build. The original strings never become part of the checkpoint.
    pub fn from_gl(
        vendor: &str,
        renderer: &str,
        version_suffix: &str,
        gl_major: u32,
        gl_minor: u32,
        requested_msaa: u16,
    ) -> Option<Self> {
        let identity = [vendor, renderer, version_suffix].map(str::to_ascii_lowercase);
        let mentions = |text: &str| identity.iter().any(|part| part.contains(text));
        let renderer_family = if mentions("mesa") || mentions("llvmpipe") || mentions("softpipe") {
            RendererFamily::Mesa
        } else if mentions("intel") {
            RendererFamily::Intel
        } else if mentions("nvidia") {
            RendererFamily::Nvidia
        } else if mentions("amd") || mentions("radeon") || mentions("ati technologies") {
            RendererFamily::Amd
        } else {
            RendererFamily::Other
        };
        let driver_build =
            if renderer_family == RendererFamily::Intel && identity[0].contains("intel") {
                intel_build(version_suffix)
            } else {
                None
            };
        let checkpoint = Self {
            renderer_family,
            gl_major,
            gl_minor,
            requested_msaa,
            driver_build,
        };
        checkpoint.valid().then_some(checkpoint)
    }

    /// Save once in the exact run directory supplied by the supervisor.
    pub fn write(self, report_dir: &Path) -> io::Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(report_dir.join(CHECKPOINT))?;
        serde_json::to_writer(&file, &self).map_err(io::Error::other)?;
        file.sync_all()
    }

    pub(super) fn read(report_dir: &Path) -> Option<Self> {
        let file = File::open(report_dir.join(CHECKPOINT)).ok()?;
        serde_json::from_reader(file.take(CHECKPOINT_LIMIT))
            .ok()
            .filter(Self::valid)
    }

    pub(super) fn from_report(value: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(value.clone())
            .ok()
            .filter(Self::valid)
    }

    fn valid(&self) -> bool {
        (1..=9).contains(&self.gl_major)
            && self.gl_minor <= 9
            && matches!(self.requested_msaa, 0 | 2 | 4 | 8)
            && (self.driver_build.is_none() || self.renderer_family == RendererFamily::Intel)
    }
}

fn intel_build(version_suffix: &str) -> Option<[u16; 4]> {
    let mut words = version_suffix.split_ascii_whitespace();
    words.find(|word| word.eq_ignore_ascii_case("build"))?;
    let mut parts = words.next()?.split('.');
    let mut next = || {
        let part = parts.next()?;
        if part.len() > 5 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let build = [next()?, next()?, next()?, next()?];
    parts.next().is_none().then_some(build)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crash_report::tests::TempDir;
    use serde_json::json;
    use std::fs;

    #[test]
    fn driver_identifiers_are_reduced_to_allowlisted_fields() {
        for (vendor, renderer, suffix, expected, build) in [
            (
                "Intel",
                "Intel UHD private-device",
                "Build 30.0.101.3118",
                "Intel",
                Some([30, 0, 101, 3118]),
            ),
            (
                "Intel",
                "private-device",
                "Mesa 25.3.0 Build 30.0.101.3118",
                "Mesa",
                None,
            ),
            ("Microsoft", "llvmpipe", "Build 30.0.101.3118", "Mesa", None),
            (
                "ATI Technologies Inc.",
                "private-device",
                "private-driver",
                "AMD",
                None,
            ),
            (
                "NVIDIA Corporation",
                "private-device",
                "private-driver",
                "Nvidia",
                None,
            ),
            (
                "private-vendor",
                "private-device",
                "Build 30.0.101.3118",
                "Other",
                None,
            ),
        ] {
            let checkpoint =
                GraphicsCheckpoint::from_gl(vendor, renderer, suffix, 4, 6, 4).unwrap();
            let mut expected = json!({
                "renderer_family": expected, "gl_major": 4, "gl_minor": 6, "requested_msaa": 4,
            });
            if let Some(build) = build {
                expected["driver_build"] = json!(build);
            }
            assert_eq!(serde_json::to_value(checkpoint).unwrap(), expected);
        }
    }

    #[test]
    fn malformed_or_unknown_intel_builds_are_omitted() {
        for suffix in [
            "30.0.101.3118",
            "Build",
            "Build 30.0.101",
            "Build 30.0.101.3118.1",
            "Build 30.0.101.65536",
            "Build 30.0.101.+3118",
            "Build 30.0.101.private",
        ] {
            let checkpoint = GraphicsCheckpoint::from_gl("Intel", "", suffix, 4, 6, 4).unwrap();
            assert_eq!(checkpoint.driver_build, None);
        }
    }

    #[test]
    fn old_or_malformed_checkpoints_are_ignored() {
        let temp = TempDir::new();
        let dir = &temp.0;
        assert!(GraphicsCheckpoint::read(dir).is_none());
        for malformed in [
            json!("unstructured driver value"),
            json!({"renderer_family": "C:/private/driver", "gl_major": 4, "gl_minor": 6, "requested_msaa": 4}),
            json!({"renderer_family": "Intel", "gl_major": 100, "gl_minor": 6, "requested_msaa": 4}),
            json!({"renderer_family": "Intel", "gl_major": 4, "gl_minor": 6, "requested_msaa": 3}),
            json!({"renderer_family": "Intel", "gl_major": 4, "gl_minor": 6, "requested_msaa": 4, "driver_build": "private-driver"}),
            json!({"renderer_family": "Mesa", "gl_major": 4, "gl_minor": 6, "requested_msaa": 4, "driver_build": [30, 0, 101, 3118]}),
        ] {
            fs::write(dir.join(CHECKPOINT), malformed.to_string()).unwrap();
            assert!(GraphicsCheckpoint::read(dir).is_none());
            assert!(GraphicsCheckpoint::from_report(&malformed).is_none());
        }
        fs::write(dir.join(CHECKPOINT), b"{").unwrap();
        assert!(GraphicsCheckpoint::read(dir).is_none());
    }
}
