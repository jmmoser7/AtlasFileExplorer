//! Present filesystem paths to external tools without changing their ownership.
use std::{borrow::Cow, path::Path};

pub(super) fn external_tool_path(path: &Path) -> Cow<'_, str> {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::VerbatimDisk(_) => {
                    if let Some(plain) = text.strip_prefix(r"\\?\") {
                        return Cow::Owned(plain.to_owned());
                    }
                }
                Prefix::VerbatimUNC(_, _) => {
                    if let Some(plain) = text.strip_prefix(r"\\?\UNC\") {
                        return Cow::Owned(format!(r"\\{plain}"));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_tool_spelling_preserves_native_path_data() {
        #[cfg(windows)]
        let cases = [
            (
                r"\\?\C:\études\dashboard files\output",
                r"C:\études\dashboard files\output",
            ),
            (
                r"\\?\UNC\server\share\études\output",
                r"\\server\share\études\output",
            ),
            (r"C:\ordinary\output", r"C:\ordinary\output"),
            (r"\\?\Volume{example}\output", r"\\?\Volume{example}\output"),
        ];
        #[cfg(not(windows))]
        let cases = [
            (
                "/tmp/études/dashboard files/output",
                "/tmp/études/dashboard files/output",
            ),
            (r"\\?\C:\literal-linux-name", r"\\?\C:\literal-linux-name"),
        ];
        for (source, expected) in cases {
            assert_eq!(external_tool_path(Path::new(source)), expected);
        }
    }
}
