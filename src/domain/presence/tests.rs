//! Tests for the parent module.

use super::*;
use crate::domain::event::{CursorPosition, Environment, Group};
use crate::domain::presence::buttons::{KRAD_BUTTON_LABEL, MAX_BUTTONS, REPO_BUTTON_LABEL};
use crate::domain::presence::icons::{FALLBACK_IMAGE, ICON_BASE_URL};
use crate::domain::presence::text::MAX_TEXT_CHARS;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

fn state() -> State {
    let mut state = State::new(UNIX_EPOCH);
    state.environment = Some(Environment {
        id: "env".to_owned(),
        name: "Krad ADE".to_owned(),
    });
    state.groups = vec![Group {
        id: "1".to_owned(),
        name: "api".to_owned(),
        root: PathBuf::from("/code/api"),
    }];
    state.active_file = Some(PathBuf::from("/code/api/src/main.rs"));
    state.editor_focused = true;
    state.cursor = Some(CursorPosition {
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
    state.groups.push(Group {
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

fn agent(harness: Option<&str>, group: Option<&str>, state: AgentState) -> AgentSnapshot {
    agent_at(harness, group, state, 0)
}

fn agent_at(
    harness: Option<&str>,
    group: Option<&str>,
    state: AgentState,
    seconds: u64,
) -> AgentSnapshot {
    AgentSnapshot {
        harness: harness.map(str::to_owned),
        group: group.map(str::to_owned),
        state,
        updated_at: at(seconds),
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
    state.groups.push(Group {
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
fn a_chat_shows_the_agent_icon_instead_of_the_language() {
    let mut state = state();
    state.editor_focused = false;
    state.agents.insert(
        "t-1".to_owned(),
        agent(Some("ClaudeCode"), Some("1"), AgentState::Working),
    );
    let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
    assert_eq!(presence.state, "main.rs · 1 agent working");
    assert_eq!(presence.small_image, Some(icon_url("claude")));
    assert_eq!(presence.small_text.as_deref(), Some("Claude Code working"));
}

#[test]
fn the_chat_icon_prefers_a_waiting_agent() {
    let mut state = state();
    state.editor_focused = false;
    state.agents.insert(
        "t-1".to_owned(),
        agent_at(Some("Codex"), Some("1"), AgentState::Working, 20),
    );
    state.agents.insert(
        "t-2".to_owned(),
        agent_at(Some("ClaudeCode"), Some("1"), AgentState::WaitingInput, 10),
    );
    let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
    assert_eq!(
        presence.small_image,
        Some(icon_url("claude")),
        "the agent that needs you owns the icon, even if it moved first"
    );
    assert_eq!(
        presence.small_text.as_deref(),
        Some("Claude Code waiting for input")
    );
}

#[test]
fn between_equal_states_the_freshest_agent_wins() {
    let mut state = state();
    state.editor_focused = false;
    state.agents.insert(
        "t-1".to_owned(),
        agent_at(Some("Opencode"), Some("1"), AgentState::Working, 10),
    );
    state.agents.insert(
        "t-2".to_owned(),
        agent_at(Some("Codex"), Some("1"), AgentState::Working, 20),
    );
    let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
    assert_eq!(presence.small_image, Some(icon_url("agent")));
    assert_eq!(presence.small_text.as_deref(), Some("Codex working"));
}

#[test]
fn ignored_groups_never_own_the_chat_icon() {
    let mut state = state();
    state.editor_focused = false;
    state.groups.push(Group {
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
    let presence = view(&state, &settings, UNIX_EPOCH).expect("presence");
    assert_eq!(presence.small_image, Some(icon_url("rust")));
    assert_eq!(presence.small_text.as_deref(), Some("Rust"));
}

#[test]
fn idle_clears_the_chat_icon() {
    let settings = Settings {
        idle_minutes: 5,
        ..Settings::default()
    };
    let mut state = state();
    state.editor_focused = false;
    state.last_activity = at(1_000);
    state.agents.insert(
        "t-1".to_owned(),
        agent_at(Some("ClaudeCode"), Some("1"), AgentState::Idle, 1_000),
    );
    let presence = view(&state, &settings, at(9_999)).expect("presence");
    assert_eq!(presence.small_image, Some(icon_url(IDLE_IMAGE)));
    assert_eq!(presence.small_text.as_deref(), Some(IDLE));
}

#[test]
fn show_agents_off_keeps_the_language_icon_in_a_chat() {
    let mut state = state();
    state.editor_focused = false;
    state.agents.insert(
        "t-1".to_owned(),
        agent(Some("ClaudeCode"), Some("1"), AgentState::Working),
    );
    let settings = Settings {
        show_agents: false,
        ..Settings::default()
    };
    let presence = view(&state, &settings, UNIX_EPOCH).expect("presence");
    assert_eq!(presence.small_image, Some(icon_url("rust")));
    assert_eq!(presence.small_text.as_deref(), Some("Rust"));
}

#[test]
fn a_chat_without_active_agents_keeps_the_language_icon() {
    let mut state = state();
    state.editor_focused = false;
    state.agents.insert(
        "t-1".to_owned(),
        agent(Some("Codex"), Some("1"), AgentState::Idle),
    );
    let presence = view(&state, &Settings::default(), UNIX_EPOCH).expect("presence");
    assert_eq!(presence.small_image, Some(icon_url("rust")));
    assert_eq!(presence.small_text.as_deref(), Some("Rust"));
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
