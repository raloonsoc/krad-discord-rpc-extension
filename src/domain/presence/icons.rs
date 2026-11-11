//! The Discord asset catalogue: language and harness icons, as URLs.

use std::ffi::OsStr;
use std::path::Path;

/// Asset key of the large "Krad ADE" image in the Discord Developer Portal.
pub(crate) const LARGE_IMAGE: &str = "krad";
/// Asset key used when the file extension has no icon of its own.
pub(crate) const FALLBACK_IMAGE: &str = "file";
/// Asset key shown while the workspace has been idle.
pub(crate) const IDLE_IMAGE: &str = "idle";

/// Small images are sent as HTTPS URLs, not asset keys: Discord's client cache
/// does not resolve freshly uploaded keys, while external URLs always work
/// (it turns them into `mp:external/...`). The repository must stay public.
pub(crate) const ICON_BASE_URL: &str =
    "https://raw.githubusercontent.com/raloonsoc/krad-discord-rpc-extension/main/assets/discord/";

/// Extension → (asset key, hover label). The key also names the PNG under
/// `assets/discord`.
const LANGUAGES: &[(&[&str], &str, &str)] = &[
    (&["rs"], "rust", "Rust"),
    (&["ts", "mts", "cts"], "typescript", "TypeScript"),
    (&["tsx", "jsx"], "react", "React"),
    (&["js", "mjs", "cjs"], "javascript", "JavaScript"),
    (&["py", "pyi"], "python", "Python"),
    (&["go"], "go", "Go"),
    (&["md", "mdx"], "markdown", "Markdown"),
    (&["toml"], "toml", "TOML"),
    (&["json", "jsonc"], "json", "JSON"),
    (&["yaml", "yml"], "yaml", "YAML"),
    (&["sh", "bash", "zsh", "fish"], "shell", "Shell"),
    (&["c", "h"], "c", "C"),
    (&["cc", "cpp", "cxx", "hpp", "hh"], "cpp", "C++"),
    (&["java"], "java", "Java"),
    (&["kt", "kts"], "kotlin", "Kotlin"),
    (&["swift"], "swift", "Swift"),
    (&["html", "htm"], "html", "HTML"),
    (&["css"], "css", "CSS"),
    (&["scss", "sass"], "sass", "Sass"),
    (&["php"], "php", "PHP"),
    (&["rb"], "ruby", "Ruby"),
    (&["cs"], "csharp", "C#"),
    (&["sql"], "sql", "SQL"),
    (&["vue"], "vue", "Vue"),
    (&["svelte"], "svelte", "Svelte"),
    (&["lua"], "lua", "Lua"),
    (&["zig"], "zig", "Zig"),
    (&["ex", "exs"], "elixir", "Elixir"),
];

/// The URL Discord loads a small icon from.
#[must_use]
pub(crate) fn icon_url(asset: &str) -> String {
    format!("{ICON_BASE_URL}{asset}.png")
}

/// Asset key and hover label for a file, from its extension.
pub(crate) fn language_icon(path: &Path) -> (&'static str, &'static str) {
    let extension = path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    extension
        .as_deref()
        .and_then(|extension| {
            LANGUAGES
                .iter()
                .find(|(extensions, _, _)| extensions.contains(&extension))
        })
        .map_or((FALLBACK_IMAGE, "File"), |(_, key, label)| (*key, *label))
}

/// Asset key and display name for a harness.
///
/// Matching ignores case, spaces, `-` and `_`, so `Claude Code`, `ClaudeCode`
/// and `claude-code` share an icon. Brand icons come from Simple Icons;
/// `agent` is the extension's own fallback.
pub(crate) fn harness_icon(harness: Option<&str>) -> (&'static str, String) {
    let Some(harness) = harness else {
        return generic_agent_icon();
    };
    let normalized: String = harness
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '-' && *character != '_')
        .flat_map(char::to_lowercase)
        .collect();
    if let Some(id) = normalized.strip_prefix("acp:") {
        return acp_icon(id, acp_id(harness));
    }
    match normalized.as_str() {
        "claudecode" => ("claude", "Claude Code".to_owned()),
        "opencode" => ("opencode", "opencode".to_owned()),
        "codex" => ("agent", "Codex".to_owned()),
        "shell" => ("shell", "Shell".to_owned()),
        _ => generic_agent_icon(),
    }
}

/// The id after `Acp:`, in its original spelling.
fn acp_id(harness: &str) -> &str {
    harness.split_once(':').map_or(harness, |(_, id)| id).trim()
}

/// Known ACP agents, matched by a substring of their id. An unknown id keeps
/// its own name so a new agent is still identifiable.
fn acp_icon(id: &str, original: &str) -> (&'static str, String) {
    let brand = [
        ("gemini", "googlegemini", "Gemini"),
        ("copilot", "githubcopilot", "Copilot"),
        ("cursor", "cursor", "Cursor"),
        ("mistral", "mistralai", "Mistral"),
        ("ollama", "ollama", "Ollama"),
        ("claude", "claude", "Claude"),
    ]
    .into_iter()
    .find(|(needle, _, _)| id.contains(needle));
    brand.map_or_else(
        || ("agent", original.to_owned()),
        |(_, asset, name)| (asset, name.to_owned()),
    )
}

fn generic_agent_icon() -> (&'static str, String) {
    ("agent", "Agent".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_extension_has_its_asset_and_the_rest_fall_back() {
        let cases = [
            ("a.rs", "rust"),
            ("a.ts", "typescript"),
            ("a.tsx", "react"),
            ("a.js", "javascript"),
            ("a.py", "python"),
            ("a.go", "go"),
            ("a.md", "markdown"),
            ("a.toml", "toml"),
            ("a.json", "json"),
            ("a.yaml", "yaml"),
            ("a.sh", "shell"),
            ("a.c", "c"),
            ("a.cpp", "cpp"),
            ("a.java", "java"),
            ("a.kt", "kotlin"),
            ("a.swift", "swift"),
            ("a.html", "html"),
            ("a.css", "css"),
            ("a.rb", "ruby"),
            ("a.cs", "csharp"),
            ("a.sql", "sql"),
            ("a.vue", "vue"),
            ("a.unknown", FALLBACK_IMAGE),
            ("Makefile", FALLBACK_IMAGE),
        ];
        for (file, expected) in cases {
            assert_eq!(
                language_icon(Path::new(file)).0,
                expected,
                "{file} should use {expected}"
            );
        }
    }

    #[test]
    fn the_file_icon_is_case_insensitive() {
        assert_eq!(language_icon(Path::new("MAIN.RS")).0, "rust");
    }

    #[test]
    fn harnesses_map_to_their_icon_and_name() {
        let cases = [
            (Some("Claude Code"), "claude", "Claude Code"),
            (Some("ClaudeCode"), "claude", "Claude Code"),
            (Some("claude-code"), "claude", "Claude Code"),
            (Some("opencode"), "opencode", "opencode"),
            (Some("OPENCODE"), "opencode", "opencode"),
            (Some("Codex"), "agent", "Codex"),
            (Some("Shell"), "shell", "Shell"),
            (Some("Gateway"), "agent", "Agent"),
            (Some("something-else"), "agent", "Agent"),
            (None, "agent", "Agent"),
        ];
        for (harness, asset, name) in cases {
            assert_eq!(
                harness_icon(harness),
                (asset, name.to_owned()),
                "{harness:?} should map to {asset} ({name})"
            );
        }
    }

    #[test]
    fn acp_harnesses_pick_a_brand_icon_and_keep_unknown_ids() {
        let cases = [
            ("Acp:gemini-cli", "googlegemini", "Gemini"),
            ("Acp:github-copilot", "githubcopilot", "Copilot"),
            ("acp:cursor-agent", "cursor", "Cursor"),
            ("Acp:mistral-vibe", "mistralai", "Mistral"),
            ("Acp:ollama", "ollama", "Ollama"),
            ("Acp:claude-code", "claude", "Claude"),
            ("Acp:MyAgent", "agent", "MyAgent"),
        ];
        for (harness, asset, name) in cases {
            assert_eq!(
                harness_icon(Some(harness)),
                (asset, name.to_owned()),
                "{harness} should map to {asset} ({name})"
            );
        }
    }

    #[test]
    fn every_icon_key_has_its_png_in_the_assets_folder() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/discord");
        let mut keys: Vec<&str> = LANGUAGES.iter().map(|(_, key, _)| *key).collect();
        keys.extend([FALLBACK_IMAGE, IDLE_IMAGE]);
        let harnesses = [
            None,
            Some("Claude Code"),
            Some("opencode"),
            Some("Codex"),
            Some("Shell"),
            Some("Gateway"),
            Some("Acp:gemini"),
            Some("Acp:copilot"),
            Some("Acp:cursor"),
            Some("Acp:mistral"),
            Some("Acp:ollama"),
            Some("Acp:claude"),
            Some("Acp:unknown"),
        ];
        keys.extend(harnesses.into_iter().map(|harness| harness_icon(harness).0));
        for key in keys {
            let path = folder.join(format!("{key}.png"));
            assert!(
                path.is_file(),
                "{} is missing; small icons are loaded by URL",
                path.display()
            );
        }
    }
}
