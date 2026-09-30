use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use std::{fs, path::Path};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawConfig {
    version: u32,
    #[serde(default)]
    ignore_class_patterns: Vec<String>,
    #[serde(default)]
    exclude_files: Vec<String>,
}
pub struct Config {
    pub classes: GlobSet,
    pub files: GlobSet,
}
impl Config {
    pub fn load(root: &Path) -> Result<Self, String> {
        let raw = match fs::read_to_string(root.join(".inheriteame.json")) {
            Ok(text) => serde_json::from_str::<RawConfig>(&text)
                .map_err(|e| format!("Invalid .inheriteame.json: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => RawConfig {
                version: 1,
                ignore_class_patterns: vec![],
                exclude_files: vec![],
            },
            Err(e) => return Err(format!("Cannot read .inheriteame.json: {e}")),
        };
        if raw.version != 1 {
            return Err("Unsupported .inheriteame.json version; expected 1".into());
        }
        Ok(Self {
            classes: compile(&raw.ignore_class_patterns, true)?,
            files: compile(&raw.exclude_files, false)?,
        })
    }
}
fn compile(patterns: &[String], classes: bool) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if pattern.is_empty()
            || pattern.contains('\\')
            || pattern.starts_with('/')
            || pattern.contains(':')
            || pattern.split('/').any(|part| part == ".." || part == ".")
            || (classes && pattern.contains('/'))
        {
            return Err(format!("Invalid exclusion pattern: {pattern:?}"));
        }
        builder.add(
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .case_insensitive(classes)
                .backslash_escape(false)
                .build()
                .map_err(|e| format!("Invalid pattern {pattern:?}: {e}"))?,
        );
    }
    builder.build().map_err(|e| e.to_string())
}
