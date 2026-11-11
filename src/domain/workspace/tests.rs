//! Tests for the parent module.

use super::*;
use crate::domain::agents::{AgentSnapshot, AgentState};
use std::path::Path;
use std::time::Duration;

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn environment() -> Environment {
    Environment {
        id: "env-1".to_owned(),
        name: "Krad ADE".to_owned(),
    }
}

fn group(id: &str, name: &str, root: &str) -> Group {
    Group {
        id: id.to_owned(),
        name: name.to_owned(),
        root: PathBuf::from(root),
    }
}

fn context() -> WorkspaceContext {
    WorkspaceContext {
        environment: Some(environment()),
        groups: vec![
            group("1", "api", "/code/api"),
            group("2", "api-web", "/code/api/web"),
        ],
        active_file: Some(PathBuf::from("/code/api/web/src/main.rs")),
        cursor: Some(CursorPosition {
            line: 42,
            column: 7,
        }),
    }
}

#[test]
fn the_context_fills_every_workspace_field() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.load(&context(), at(10));
    assert_eq!(state.environment, Some(environment()));
    assert_eq!(state.groups.len(), 2);
    assert_eq!(
        state.active_file.as_deref(),
        Some(Path::new("/code/api/web/src/main.rs"))
    );
    assert_eq!(state.cursor.map(|cursor| cursor.line), Some(42));
    assert!(state.editor_focused, "a file means the editor has focus");
    assert!(state.agents.is_empty());
    assert!(!state.paused);
    assert_eq!(state.session_start, SystemTime::UNIX_EPOCH);
    assert_eq!(state.last_activity, at(10), "loading counts as activity");
}

#[test]
fn workspace_events_update_the_state() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.load(&context(), at(10));

    state.apply(
        &Event::FileFocused(Some(PathBuf::from("/code/api/lib.rs"))),
        at(20),
    );
    assert_eq!(
        state.active_file.as_deref(),
        Some(Path::new("/code/api/lib.rs"))
    );
    assert_eq!(state.cursor, None, "a new file has an unknown cursor");
    assert!(state.editor_focused);
    assert_eq!(state.last_activity, at(20));

    state.apply(&Event::FileSaved(PathBuf::from("/code/api/lib.rs")), at(30));
    assert_eq!(
        state.last_saved.as_deref(),
        Some(Path::new("/code/api/lib.rs"))
    );

    state.apply(
        &Event::GroupsChanged(vec![group("3", "other", "/code/other")]),
        at(40),
    );
    assert_eq!(state.groups, vec![group("3", "other", "/code/other")]);

    let before = state.active_file.clone();
    assert!(before.is_some());
    state.apply(&Event::FileFocused(None), at(50));
    assert_eq!(
        state.active_file, before,
        "focus leaving keeps the last file"
    );
    assert!(
        !state.editor_focused,
        "no active file means no editor focus"
    );
    assert_eq!(state.cursor, None);
    assert_eq!(state.last_activity, at(50));
}

#[test]
fn editor_focus_follows_the_active_file_and_comes_back() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    assert!(!state.editor_focused, "no context yet, no focus");

    state.load(&context(), at(10));
    assert!(state.editor_focused);

    state.apply(&Event::FileFocused(None), at(20));
    assert!(!state.editor_focused, "a chat or terminal took the focus");

    state.apply(
        &Event::FileFocused(Some(PathBuf::from("/code/api/lib.rs"))),
        at(30),
    );
    assert!(state.editor_focused, "back in the editor");
}

#[test]
fn a_context_without_a_file_means_no_editor_focus() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    let mut context = context();
    context.active_file = None;
    context.cursor = None;
    state.load(&context, at(10));
    assert!(!state.editor_focused);
}

#[test]
fn closing_the_current_environment_clears_the_workspace() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.load(&context(), at(10));

    let mut other = environment();
    other.id = "env-2".to_owned();
    state.apply(&Event::EnvironmentClosed(other), at(20));
    assert_eq!(state.environment, Some(environment()));
    assert!(state.active_file.is_some());

    state.apply(&Event::EnvironmentClosed(environment()), at(30));
    assert_eq!(state.environment, None);
    assert_eq!(state.active_file, None);
    assert!(!state.editor_focused);
    assert_eq!(state.cursor, None);
}

#[test]
fn opening_an_environment_replaces_it() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    let mut opened = environment();
    opened.id = "env-9".to_owned();
    opened.name = "other".to_owned();
    state.apply(&Event::EnvironmentOpened(opened.clone()), at(10));
    assert_eq!(state.environment, Some(opened));
}

#[test]
fn the_innermost_group_holding_the_file_wins() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.load(&context(), at(10));
    assert_eq!(
        state.active_group().map(|group| group.name.as_str()),
        Some("api-web")
    );

    state.apply(
        &Event::FileFocused(Some(PathBuf::from("/code/api/src/main.rs"))),
        at(20),
    );
    assert_eq!(
        state.active_group().map(|group| group.name.as_str()),
        Some("api")
    );
}

#[test]
fn paths_that_only_share_a_prefix_do_not_match() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.groups = vec![group("1", "api", "/code/api")];
    state.active_file = Some(PathBuf::from("/code/api2/main.rs"));
    assert_eq!(state.active_group(), None);
}

#[test]
fn without_a_file_a_single_group_is_the_project() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.groups = vec![group("1", "api", "/code/api")];
    assert_eq!(
        state.active_group().map(|group| group.name.as_str()),
        Some("api")
    );

    state.groups.push(group("2", "web", "/code/web"));
    assert_eq!(state.active_group(), None, "several groups are ambiguous");
}

#[test]
fn commands_count_as_activity() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    assert_eq!(state.last_activity, SystemTime::UNIX_EPOCH);
    state.mark_activity(at(42));
    assert_eq!(state.last_activity, at(42));
}

#[test]
fn pause_flips_both_ways() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    assert!(state.toggle_paused());
    assert!(state.paused);
    assert!(!state.toggle_paused());
    assert!(!state.paused);
}

fn agent_started(harness: Option<&str>) -> Event {
    Event::AgentStarted {
        terminal: "t-1".to_owned(),
        harness: harness.map(str::to_owned),
        group: Some("2".to_owned()),
    }
}

#[test]
fn agent_events_fill_the_map_and_count_as_activity() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.apply(&agent_started(Some("ClaudeCode")), at(10));
    assert_eq!(
        state.agents.get("t-1"),
        Some(&AgentSnapshot {
            harness: Some("ClaudeCode".to_owned()),
            group: Some("2".to_owned()),
            state: AgentState::Working,
            updated_at: at(10),
        })
    );
    assert_eq!(state.last_activity, at(10), "agent events are activity");

    state.apply(
        &Event::AgentWaiting {
            terminal: "t-1".to_owned(),
            group: Some("2".to_owned()),
        },
        at(20),
    );
    assert_eq!(
        state.agents.get("t-1").map(|agent| agent.state),
        Some(AgentState::WaitingInput)
    );
    assert_eq!(
        state
            .agents
            .get("t-1")
            .and_then(|agent| agent.harness.as_deref()),
        Some("ClaudeCode"),
        "waiting keeps what the start announced"
    );
    assert_eq!(state.last_activity, at(20));
    assert_eq!(
        state.agents.get("t-1").map(|agent| agent.updated_at),
        Some(at(20)),
        "every state change refreshes the timestamp"
    );
}

#[test]
fn a_finished_agent_stays_idle_with_its_harness() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.apply(&agent_started(Some("Codex")), at(10));
    state.apply(
        &Event::AgentFinished {
            terminal: "t-1".to_owned(),
            group: Some("2".to_owned()),
        },
        at(20),
    );
    assert_eq!(
        state.agents.get("t-1"),
        Some(&AgentSnapshot {
            harness: Some("Codex".to_owned()),
            group: Some("2".to_owned()),
            state: AgentState::Idle,
            updated_at: at(20),
        }),
        "finished agents stay, they are just not active"
    );
}

#[test]
fn events_for_unknown_terminals_do_not_panic() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.apply(
        &Event::AgentWaiting {
            terminal: "gone".to_owned(),
            group: Some("2".to_owned()),
        },
        at(10),
    );
    state.apply(
        &Event::AgentFinished {
            terminal: "gone".to_owned(),
            group: Some("2".to_owned()),
        },
        at(20),
    );
    assert_eq!(
        state.agents.get("gone"),
        Some(&AgentSnapshot {
            harness: None,
            group: Some("2".to_owned()),
            state: AgentState::Idle,
            updated_at: at(20),
        })
    );

    state.apply(&Event::SessionClosed, at(30));
    assert_eq!(
        state.last_activity,
        at(20),
        "sessions are not agent activity"
    );
}

#[test]
fn non_agent_events_leave_the_agents_and_the_clock_alone() {
    let mut state = State::new(SystemTime::UNIX_EPOCH);
    state.apply(&Event::GitChanged, at(10));
    assert!(state.agents.is_empty());
    assert_eq!(state.last_activity, SystemTime::UNIX_EPOCH);
}
