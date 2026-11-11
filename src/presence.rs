//! Turns workspace state plus settings into what Discord should show.
//!
//! This module is pure: no app, no Discord, no clock of its own and no
//! filesystem. The caller injects the current time so the idle timeout is
//! testable, and the Discord thread receives the result over a channel.

use crate::settings::Settings;
use crate::state::{AgentSnapshot, AgentState, State};
use krad_extension_sdk::api::protocol::FilePosition;
use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Asset key of the large "Krad ADE" image in the Discord Developer Portal.
pub const LARGE_IMAGE: &str = "krad";
/// Asset key used when the file extension has no icon of its own.
pub const FALLBACK_IMAGE: &str = "file";
/// Asset key shown while the workspace has been idle.
pub const IDLE_IMAGE: &str = "idle";

/// Small images are sent as HTTPS URLs, not asset keys: Discord's client cache
/// does not resolve freshly uploaded keys, while external URLs always work
/// (it turns them into `mp:external/...`). The repository must stay public.
pub const ICON_BASE_URL: &str =
    "https://raw.githubusercontent.com/raloonsoc/krad-discord-rpc-extension/main/assets/discord/";

/// Discord cuts details, state and the two asset texts at 128 characters.
pub const MAX_TEXT_CHARS: usize = 128;

/// Discord accepts at most two buttons per activity.
pub const MAX_BUTTONS: usize = 2;
pub const MAX_BUTTON_LABEL: usize = 32;
pub const MAX_BUTTON_URL: usize = 512;

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

pub const KRAD_BUTTON_LABEL: &str = "Get Krad ADE";
pub const KRAD_BUTTON_URL: &str = "https://github.com/joordih/krad-ade";
pub const REPO_BUTTON_LABEL: &str = "View repository";

const APP_NAME: &str = "Krad ADE";
const IDLE: &str = "Idle";

/// One of the links Discord shows under the activity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceButton {
    pub label: String,
    pub url: String,
}

/// What to send to Discord, already detached from the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceView {
    pub details: String,
    pub state: String,
    pub large_image: String,
    pub large_text: String,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
    pub buttons: Vec<PresenceButton>,
    /// Unix seconds, the unit Discord expects.
    pub start_timestamp: i64,
}

/// The presence for this state and these settings at `now`, or `None` when it
/// must be hidden (paused, disabled, or inside an ignored group).
#[must_use]
pub fn view(state: &State, settings: &Settings, now: SystemTime) -> Option<PresenceView> {
    if state.paused || !settings.enabled {
        return None;
    }
    let group = state.active_group();
    if group.is_some_and(|group| settings.is_group_ignored(&group.name)) {
        return None;
    }

    let project = if settings.show_project {
        group
            .map(|group| group.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                state
                    .environment
                    .as_ref()
                    .map(|environment| environment.name.trim())
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
            })
    } else {
        None
    };
    let branch = state
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        // The branch belongs to the group's repository: next to the
        // environment name it would be a stale branch of another project.
        .filter(|_| group.is_some() && settings.show_project && settings.show_branch);
    let details = match (project, branch) {
        (Some(project), Some(branch)) => format!("{project} · {branch}"),
        (Some(project), None) => project,
        (None, Some(branch)) => format!("{APP_NAME} · {branch}"),
        (None, None) => APP_NAME.to_owned(),
    };
    let large_text = if settings.show_project {
        state
            .environment
            .as_ref()
            .map(|environment| environment.name.trim())
            .filter(|name| !name.is_empty())
            .map_or_else(
                || APP_NAME.to_owned(),
                |name| format!("{APP_NAME} · {name}"),
            )
    } else {
        APP_NAME.to_owned()
    };

    let agents = agent_summary(state, settings);
    // A working or waiting agent is activity: never flip to idle under it,
    // even when the last event is older than the timeout.
    let idle = !agents.active()
        && now.duration_since(state.last_activity).unwrap_or_default()
            >= Duration::from_secs(u64::from(settings.idle_minutes) * 60);
    let active_file = state
        .active_file
        .as_deref()
        .filter(|_| settings.show_file && !idle);
    let (state_line, icon) = if idle {
        (IDLE.to_owned(), Some((IDLE_IMAGE, IDLE)))
    } else if let Some(path) = active_file {
        let mut line = file_label(path, state.cursor, settings.show_line);
        if settings.show_agents {
            if let Some(agents) = agents.line() {
                line.push_str(" · ");
                line.push_str(&agents);
            }
        }
        (line, Some(language_icon(path)))
    } else if settings.show_agents {
        agents
            .line()
            .map_or_else(|| saved_or_idle(state, settings), |agents| (agents, None))
    } else {
        saved_or_idle(state, settings)
    };
    let small_text = if settings.show_agents {
        agents
            .single_harness_text()
            .or_else(|| icon.map(|(_, label)| label.to_owned()))
    } else {
        icon.map(|(_, label)| label.to_owned())
    };

    Some(PresenceView {
        details: truncate(details),
        state: truncate(state_line),
        large_image: LARGE_IMAGE.to_owned(),
        large_text: truncate(large_text),
        small_image: icon.map(|(asset, _)| icon_url(asset)),
        small_text: small_text.map(truncate),
        buttons: buttons(state, settings),
        start_timestamp: unix_seconds(state.session_start),
    })
}

/// The last saved file fills the gap when no file is open, or plain idle.
fn saved_or_idle(
    state: &State,
    settings: &Settings,
) -> (String, Option<(&'static str, &'static str)>) {
    if settings.show_file {
        if let Some(path) = state.last_saved.as_deref() {
            return (format!("Saved {}", file_label(path, None, false)), None);
        }
    }
    (IDLE.to_owned(), Some((IDLE_IMAGE, IDLE)))
}

/// The URL Discord loads a small icon from.
#[must_use]
pub fn icon_url(asset: &str) -> String {
    format!("{ICON_BASE_URL}{asset}.png")
}

/// Fixed links Discord shows below the activity. Discord caps them at two and
/// silently rejects longer labels or URLs, so anything invalid is dropped here
/// instead of losing the whole update.
fn buttons(state: &State, settings: &Settings) -> Vec<PresenceButton> {
    let mut buttons = Vec::new();
    if settings.show_krad_button {
        if let Some(button) = button(KRAD_BUTTON_LABEL, KRAD_BUTTON_URL) {
            buttons.push(button);
        }
    }
    if settings.show_project && settings.show_repo_button {
        if let Some(button) = state
            .repo_url
            .as_deref()
            .and_then(|url| button(REPO_BUTTON_LABEL, url))
        {
            buttons.push(button);
        }
    }
    buttons.truncate(MAX_BUTTONS);
    buttons
}

fn button(label: &str, url: &str) -> Option<PresenceButton> {
    let label = label.trim();
    let url = url.trim();
    if label.is_empty()
        || url.is_empty()
        || label.chars().count() > MAX_BUTTON_LABEL
        || url.chars().count() > MAX_BUTTON_URL
        || !url.starts_with("https://")
    {
        return None;
    }
    Some(PresenceButton {
        label: label.to_owned(),
        url: url.to_owned(),
    })
}

fn file_label(path: &Path, cursor: Option<FilePosition>, show_line: bool) -> String {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .map_or_else(|| path.display().to_string(), str::to_owned);
    match (show_line, cursor) {
        (true, Some(position)) if position.line > 0 => format!("{name}:{}", position.line),
        _ => name,
    }
}

/// What the active agents add to the presence.
#[derive(Default)]
struct AgentSummary {
    working: usize,
    waiting: usize,
    /// Harness of the only active agent, when there is exactly one.
    only_harness: Option<String>,
}

impl AgentSummary {
    fn active(&self) -> bool {
        self.working + self.waiting > 0
    }

    fn count(&self) -> usize {
        self.working + self.waiting
    }

    /// Waiting wins over working: an agent blocked on input is the most useful
    /// thing to report. `None` while nothing is active.
    fn line(&self) -> Option<String> {
        if self.waiting > 0 {
            Some(match self.waiting {
                1 => "1 agent waiting for input".to_owned(),
                waiting => format!("{waiting} agents waiting for input"),
            })
        } else if self.working > 0 {
            Some(match self.working {
                1 => "1 agent working".to_owned(),
                working => format!("{working} agents working"),
            })
        } else {
            None
        }
    }

    /// The harness name is private enough to appear only for a lone agent.
    fn single_harness_text(&self) -> Option<String> {
        if self.count() != 1 {
            return None;
        }
        let harness = harness_label(self.only_harness.as_deref()?);
        let doing = if self.waiting == 1 {
            "waiting for input"
        } else {
            "working"
        };
        Some(format!("{harness} {doing}"))
    }
}

/// Counts the agents that are working or waiting, skipping ignored groups.
/// Finished agents stay in the map but are not active.
fn agent_summary(state: &State, settings: &Settings) -> AgentSummary {
    let mut summary = AgentSummary::default();
    let mut harnesses = Vec::new();
    for agent in state.agents.values() {
        if agent_is_ignored(state, agent, settings) {
            continue;
        }
        match agent.state {
            AgentState::Idle => continue,
            AgentState::Working => summary.working += 1,
            AgentState::WaitingInput => summary.waiting += 1,
        }
        harnesses.push(agent.harness.as_deref());
    }
    if harnesses.len() == 1 {
        summary.only_harness = harnesses[0].map(str::to_owned);
    }
    summary
}

/// Whether the agent belongs to a group the user asked to ignore. The event
/// carries a group id; only the group list knows its name.
fn agent_is_ignored(state: &State, agent: &AgentSnapshot, settings: &Settings) -> bool {
    let Some(id) = agent.group.as_deref() else {
        return false;
    };
    state
        .groups
        .iter()
        .find(|group| group.id == id)
        .is_some_and(|group| settings.is_group_ignored(&group.name))
}

/// The app serialises the harness as `ClaudeCode`, `Codex`, `Opencode`,
/// `Shell`, `Gateway` or `Acp:<id>`.
fn harness_label(harness: &str) -> &str {
    match harness {
        "ClaudeCode" => "Claude Code",
        "Opencode" => "opencode",
        other => other.strip_prefix("Acp:").map_or(other, |_| "ACP agent"),
    }
}

/// Discord rejects texts over 128 characters; cut on char boundaries and mark
/// the cut so a multi-byte character is never split.
fn truncate(mut text: String) -> String {
    if text.chars().count() > MAX_TEXT_CHARS {
        text = text.chars().take(MAX_TEXT_CHARS - 1).collect();
        text.push('…');
    }
    text
}

/// Asset key and hover label for a file, from its extension.
fn language_icon(path: &Path) -> (&'static str, &'static str) {
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

fn unix_seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::protocol::{EnvironmentInfo, GroupInfo, HostInfo};
    use std::path::PathBuf;

    fn state() -> State {
        let mut state = State::new(UNIX_EPOCH);
        state.environment = Some(EnvironmentInfo {
            id: "env".to_owned(),
            name: "Krad ADE".to_owned(),
            host: HostInfo::Local,
        });
        state.groups = vec![GroupInfo {
            id: "1".to_owned(),
            name: "api".to_owned(),
            root: PathBuf::from("/code/api"),
        }];
        state.active_file = Some(PathBuf::from("/code/api/src/main.rs"));
        state.cursor = Some(FilePosition {
            line: 42,
            column: 7,
        });
        state
    }

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn the_default_view_shows_project_file_and_language() {
        let presence = view(&state(), &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.details, "api");
        assert_eq!(presence.state, "main.rs");
        assert_eq!(presence.large_image, LARGE_IMAGE);
        assert_eq!(presence.large_text, "Krad ADE · Krad ADE");
        assert_eq!(presence.small_image, Some(icon_url("rust")));
        assert_eq!(presence.small_text.as_deref(), Some("Rust"));
        assert_eq!(presence.start_timestamp, 0);
    }

    #[test]
    fn paused_disabled_or_ignored_groups_hide_the_presence() {
        let settings = Settings::default();
        let mut paused = state();
        paused.toggle_paused();
        assert_eq!(view(&paused, &settings, UNIX_EPOCH), None);

        let mut disabled = settings.clone();
        disabled.enabled = false;
        assert_eq!(view(&state(), &disabled, UNIX_EPOCH), None);

        let mut ignored = settings;
        ignored.ignored_groups = vec!["api".to_owned()];
        assert_eq!(view(&state(), &ignored, UNIX_EPOCH), None);
    }

    #[test]
    fn hiding_the_project_leaks_no_group_name() {
        let settings = Settings {
            show_project: false,
            ..Settings::default()
        };
        let presence = view(&state(), &settings, UNIX_EPOCH).expect("presence");
        assert_eq!(presence.details, "Krad ADE");
        assert_eq!(presence.large_text, "Krad ADE");
        assert!(!presence.details.contains("api"));
        assert!(!presence.large_text.contains("api"));
    }

    #[test]
    fn hiding_the_file_leaks_no_file_name_and_no_icon() {
        let settings = Settings {
            show_file: false,
            ..Settings::default()
        };
        let presence = view(&state(), &settings, UNIX_EPOCH).expect("presence");
        assert_eq!(presence.state, "Idle");
        // Only the generic idle icon: nothing that reveals the file's language.
        assert_eq!(presence.small_image, Some(icon_url(IDLE_IMAGE)));
        assert_eq!(presence.small_text.as_deref(), Some(IDLE));

        let mut saved = state();
        saved.active_file = None;
        saved.last_saved = Some(PathBuf::from("/code/api/secret.rs"));
        let presence = view(&saved, &settings, UNIX_EPOCH).expect("presence");
        assert!(!presence.state.contains("secret.rs"));
    }

    #[test]
    fn the_cursor_line_appears_only_when_asked_for() {
        let mut settings = Settings::default();
        assert_eq!(
            view(&state(), &settings, UNIX_EPOCH)
                .expect("presence")
                .state,
            "main.rs"
        );

        settings.show_line = true;
        assert_eq!(
            view(&state(), &settings, UNIX_EPOCH)
                .expect("presence")
                .state,
            "main.rs:42"
        );

        let mut no_cursor = state();
        no_cursor.cursor = None;
        assert_eq!(
            view(&no_cursor, &settings, UNIX_EPOCH)
                .expect("presence")
                .state,
            "main.rs"
        );
    }

    #[test]
    fn the_last_saved_file_fills_the_gap_when_nothing_is_open() {
        let mut state = state();
        state.active_file = None;
        state.cursor = None;
        state.last_saved = Some(PathBuf::from("/code/api/src/lib.rs"));
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.state, "Saved lib.rs");
        assert_eq!(presence.small_image, None);
    }

    #[test]
    fn the_project_comes_from_the_root_that_holds_the_file() {
        let mut state = state();
        state.groups.push(GroupInfo {
            id: "2".to_owned(),
            name: "api-web".to_owned(),
            root: PathBuf::from("/code/api/web"),
        });
        state.active_file = Some(PathBuf::from("/code/api/web/src/main.ts"));
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.details, "api-web");
        assert_eq!(presence.small_image, Some(icon_url("typescript")));

        state.active_file = Some(PathBuf::from("/code/api/src/main.rs"));
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .details,
            "api"
        );
    }

    #[test]
    fn without_a_group_the_environment_name_is_the_project() {
        let mut state = state();
        state.groups.clear();
        state.active_file = None;
        state.environment.as_mut().expect("environment").name = "Remote box".to_owned();
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.details, "Remote box");
        assert_eq!(presence.large_text, "Krad ADE · Remote box");
    }

    #[test]
    fn without_project_or_file_the_presence_stays_generic() {
        let mut state = state();
        state.groups.clear();
        state.active_file = None;
        state.environment = None;
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.details, "Krad ADE");
        assert_eq!(presence.state, "Idle");
    }

    #[test]
    fn the_branch_follows_the_project_and_its_setting() {
        let mut state = state();
        state.branch = Some("feature/discord".to_owned());
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.details, "api · feature/discord");

        let settings = Settings {
            show_branch: false,
            ..Settings::default()
        };
        assert_eq!(
            view(&state, &settings, UNIX_EPOCH)
                .expect("presence")
                .details,
            "api"
        );
    }

    #[test]
    fn a_stale_branch_never_follows_a_name_that_is_not_a_project() {
        let mut state = state();
        state.groups.clear();
        state.active_file = None;
        state.branch = Some("main".to_owned());
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .details,
            "Krad ADE"
        );
        state.environment = None;
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .details,
            "Krad ADE"
        );
    }

    #[test]
    fn hiding_the_project_hides_the_branch_and_the_repo_button() {
        let mut state = state();
        state.branch = Some("main".to_owned());
        state.repo_url = Some("https://github.com/owner/repo".to_owned());
        let settings = Settings {
            show_project: false,
            show_repo_button: true,
            ..Settings::default()
        };
        let presence = view(&state, &settings, UNIX_EPOCH).expect("presence");
        assert_eq!(presence.details, "Krad ADE");
        assert!(!presence.details.contains("main"));
        assert_eq!(presence.buttons.len(), 1, "only the fixed Krad button");
        assert_eq!(presence.buttons[0].label, KRAD_BUTTON_LABEL);
    }

    #[test]
    fn idle_starts_after_the_configured_minutes_and_activity_restores_it() {
        let settings = Settings {
            idle_minutes: 5,
            ..Settings::default()
        };
        let mut state = state();
        state.last_activity = at(1_000);

        let active = view(&state, &settings, at(1_299)).expect("presence");
        assert_eq!(active.state, "main.rs");
        assert_eq!(active.small_image, Some(icon_url("rust")));

        let idle = view(&state, &settings, at(1_300)).expect("presence");
        assert_eq!(idle.state, "Idle");
        assert_eq!(idle.small_image, Some(icon_url(IDLE_IMAGE)));
        assert_eq!(idle.small_text.as_deref(), Some("Idle"));
        assert_eq!(
            idle.start_timestamp, active.start_timestamp,
            "the session timer keeps counting"
        );

        state.mark_activity(at(1_800));
        let resumed = view(&state, &settings, at(1_800)).expect("presence");
        assert_eq!(resumed.state, "main.rs");
        assert_eq!(resumed.small_image, Some(icon_url("rust")));
    }

    #[test]
    fn the_fixed_krad_button_is_on_by_default() {
        let presence = view(&state(), &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(
            presence.buttons,
            vec![PresenceButton {
                label: "Get Krad ADE".to_owned(),
                url: "https://github.com/joordih/krad-ade".to_owned(),
            }]
        );
    }

    #[test]
    fn the_repo_button_is_opt_in_and_can_stand_alone() {
        let mut state = state();
        state.repo_url = Some("https://github.com/owner/repo".to_owned());

        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.buttons.len(), 1, "off by default");

        let settings = Settings {
            show_repo_button: true,
            ..Settings::default()
        };
        let presence = view(&state, &settings, UNIX_EPOCH).expect("presence");
        assert_eq!(
            presence.buttons.last(),
            Some(&PresenceButton {
                label: "View repository".to_owned(),
                url: "https://github.com/owner/repo".to_owned(),
            })
        );

        let settings = Settings {
            show_krad_button: false,
            show_repo_button: true,
            ..Settings::default()
        };
        let presence = view(&state, &settings, UNIX_EPOCH).expect("presence");
        assert_eq!(
            presence.buttons,
            vec![PresenceButton {
                label: "View repository".to_owned(),
                url: "https://github.com/owner/repo".to_owned(),
            }]
        );

        state.repo_url = None;
        assert!(view(&state, &settings, UNIX_EPOCH)
            .expect("presence")
            .buttons
            .is_empty());
    }

    #[test]
    fn buttons_that_break_discord_limits_are_dropped() {
        let long_label = "x".repeat(MAX_BUTTON_LABEL + 1);
        let long_url = format!("https://example.com/{}", "x".repeat(MAX_BUTTON_URL));
        assert_eq!(button(&long_label, KRAD_BUTTON_URL), None);
        assert_eq!(button(KRAD_BUTTON_LABEL, &long_url), None);
        assert_eq!(button(KRAD_BUTTON_LABEL, "http://example.com"), None);
        assert_eq!(button("", KRAD_BUTTON_URL), None);
        assert!(button(KRAD_BUTTON_LABEL, KRAD_BUTTON_URL).is_some());
    }

    #[test]
    fn a_view_never_carries_more_than_two_buttons() {
        let mut state = state();
        state.repo_url = Some("https://github.com/owner/repo".to_owned());
        let settings = Settings {
            show_repo_button: true,
            ..Settings::default()
        };
        let presence = view(&state, &settings, UNIX_EPOCH).expect("presence");
        assert_eq!(presence.buttons.len(), MAX_BUTTONS);
        assert_eq!(presence.buttons[0].label, KRAD_BUTTON_LABEL);
        assert_eq!(presence.buttons[1].label, REPO_BUTTON_LABEL);
    }

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

    fn agent(harness: Option<&str>, group: Option<&str>, state: AgentState) -> AgentSnapshot {
        AgentSnapshot {
            harness: harness.map(str::to_owned),
            group: group.map(str::to_owned),
            state,
        }
    }

    #[test]
    fn agents_with_a_file_append_the_count() {
        let mut state = state();
        state.agents.insert(
            "t-1".to_owned(),
            agent(Some("ClaudeCode"), Some("1"), AgentState::Working),
        );
        state.agents.insert(
            "t-2".to_owned(),
            agent(None, Some("1"), AgentState::Working),
        );
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.state, "main.rs · 2 agents working");
        assert_eq!(presence.small_image, Some(icon_url("rust")));
        assert_eq!(
            presence.small_text.as_deref(),
            Some("Rust"),
            "several agents never reveal their harness"
        );
    }

    #[test]
    fn waiting_has_priority_over_working() {
        let mut state = state();
        state.agents.insert(
            "t-1".to_owned(),
            agent(Some("ClaudeCode"), Some("1"), AgentState::Working),
        );
        state.agents.insert(
            "t-2".to_owned(),
            agent(Some("Codex"), Some("1"), AgentState::WaitingInput),
        );
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .state,
            "main.rs · 1 agent waiting for input"
        );

        state.active_file = None;
        state.cursor = None;
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .state,
            "1 agent waiting for input"
        );

        state.agents.insert(
            "t-3".to_owned(),
            agent(None, Some("1"), AgentState::WaitingInput),
        );
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .state,
            "2 agents waiting for input"
        );
    }

    #[test]
    fn agents_keep_the_presence_awake() {
        let settings = Settings {
            idle_minutes: 5,
            ..Settings::default()
        };
        let mut state = state();
        state.last_activity = at(1_000);
        state.agents.insert(
            "t-1".to_owned(),
            agent(Some("ClaudeCode"), Some("1"), AgentState::Working),
        );
        assert_eq!(
            view(&state, &settings, at(9_999)).expect("presence").state,
            "main.rs · 1 agent working",
            "a long-running agent never turns the presence idle"
        );

        state.agents.get_mut("t-1").expect("agent").state = AgentState::Idle;
        let idle = view(&state, &settings, at(9_999)).expect("presence");
        assert_eq!(idle.state, "Idle");
        assert_eq!(idle.small_image, Some(icon_url(IDLE_IMAGE)));
    }

    #[test]
    fn finished_agents_do_not_count_as_active() {
        let mut state = state();
        state.active_file = None;
        state.cursor = None;
        state.agents.insert(
            "t-1".to_owned(),
            agent(Some("Codex"), Some("1"), AgentState::Idle),
        );
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.state, "Idle");
        assert_eq!(presence.small_image, Some(icon_url(IDLE_IMAGE)));
    }

    #[test]
    fn agents_in_ignored_groups_do_not_count() {
        let mut state = state();
        state.groups.push(GroupInfo {
            id: "2".to_owned(),
            name: "secret".to_owned(),
            root: PathBuf::from("/code/secret"),
        });
        state.agents.insert(
            "t-1".to_owned(),
            agent(Some("ClaudeCode"), Some("2"), AgentState::Working),
        );
        let settings = Settings {
            ignored_groups: vec!["secret".to_owned()],
            ..Settings::default()
        };
        assert_eq!(
            view(&state, &settings, UNIX_EPOCH).expect("presence").state,
            "main.rs",
            "an ignored group's agent never shows up"
        );

        state.last_activity = at(1_000);
        let idle = view(&state, &settings, at(9_999)).expect("presence");
        assert_eq!(idle.state, "Idle", "nor keeps the presence awake");

        state.agents.insert(
            "t-2".to_owned(),
            agent(Some("Codex"), Some("1"), AgentState::Working),
        );
        assert_eq!(
            view(&state, &settings, UNIX_EPOCH).expect("presence").state,
            "main.rs · 1 agent working",
            "agents of the active, non-ignored group still count"
        );
    }

    #[test]
    fn show_agents_off_hides_the_counts_and_harness() {
        let mut state = state();
        state.agents.insert(
            "t-1".to_owned(),
            agent(Some("ClaudeCode"), Some("1"), AgentState::Working),
        );
        let settings = Settings {
            show_agents: false,
            ..Settings::default()
        };
        let presence = view(&state, &settings, UNIX_EPOCH).expect("presence");
        assert_eq!(presence.state, "main.rs");
        assert_eq!(presence.small_text.as_deref(), Some("Rust"));
    }

    #[test]
    fn a_lone_agent_shows_its_harness_in_the_small_text() {
        let mut state = state();
        state.agents.insert(
            "t-1".to_owned(),
            agent(Some("ClaudeCode"), Some("1"), AgentState::Working),
        );
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.small_text.as_deref(), Some("Claude Code working"));

        state.agents.get_mut("t-1").expect("agent").state = AgentState::WaitingInput;
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .small_text
                .as_deref(),
            Some("Claude Code waiting for input")
        );

        state.agents.insert(
            "t-2".to_owned(),
            agent(Some("Codex"), Some("1"), AgentState::Working),
        );
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .small_text
                .as_deref(),
            Some("Rust"),
            "with two agents the language label comes back"
        );
    }

    #[test]
    fn without_a_file_the_counts_fill_the_line() {
        let mut state = state();
        state.active_file = None;
        state.cursor = None;
        state.last_saved = None;
        state.agents.insert(
            "t-1".to_owned(),
            agent(None, Some("1"), AgentState::Working),
        );
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.state, "1 agent working");
        assert_eq!(presence.small_image, None);

        state.agents.insert(
            "t-2".to_owned(),
            agent(None, Some("1"), AgentState::Working),
        );
        assert_eq!(
            view(&state, &Settings::default(), UNIX_EPOCH)
                .expect("presence")
                .state,
            "2 agents working"
        );
    }

    #[test]
    fn texts_are_truncated_to_discords_limit_on_char_boundaries() {
        let mut state = state();
        state.groups[0].name = "é".repeat(300);
        state.environment.as_mut().expect("environment").name = "文".repeat(300);
        state.active_file = Some(PathBuf::from(format!("/code/api/{}.rs", "ü".repeat(300))));
        state.agents.insert(
            "t-1".to_owned(),
            agent(Some(&"漢".repeat(300)), Some("1"), AgentState::Working),
        );
        let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
        for text in [
            &presence.details,
            &presence.state,
            &presence.large_text,
            presence.small_text.as_ref().expect("small text"),
        ] {
            assert!(
                text.chars().count() <= MAX_TEXT_CHARS,
                "{text:?} is over the limit"
            );
            assert!(text.ends_with('…'), "{text:?} does not mark the cut");
        }
    }

    #[test]
    fn small_icons_are_external_urls_and_the_large_one_stays_an_asset() {
        let presence = view(&state(), &Settings::default(), UNIX_EPOCH).expect("presence");
        assert_eq!(presence.large_image, LARGE_IMAGE);
        assert_eq!(
            presence.small_image,
            Some(format!("{ICON_BASE_URL}rust.png"))
        );
        assert_eq!(icon_url(IDLE_IMAGE), format!("{ICON_BASE_URL}idle.png"));
        assert_eq!(icon_url(FALLBACK_IMAGE), format!("{ICON_BASE_URL}file.png"));
    }

    #[test]
    fn every_icon_key_has_its_png_in_the_assets_folder() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/discord");
        let mut keys: Vec<&str> = LANGUAGES.iter().map(|(_, key, _)| *key).collect();
        keys.extend([FALLBACK_IMAGE, IDLE_IMAGE]);
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
