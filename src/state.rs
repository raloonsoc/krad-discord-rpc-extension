//! What the extension knows about the workspace.
//!
//! [`State`] is a pure value: the workspace context fills it once at startup
//! and then every [`WorkspaceEvent`] updates it. Nothing here touches the app,
//! Discord or the filesystem, so all of it is unit-testable.

use krad_extension_sdk::api::protocol::{
    AppEvent, AppEventParams, EnvironmentInfo, FilePosition, GroupInfo, WorkspaceContextResult,
    WorkspaceEvent,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

/// What an agent is doing, as far as the presence cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    /// Finished, or not known to be doing anything. Not active.
    Idle,
    Working,
    WaitingInput,
}

/// One agent, keyed by its terminal, reduced to what Rich Presence shows.
///
/// `group` is the group id carried by the app event, not its name: presence
/// resolves it against [`State::groups`] only to honour `ignored-groups` and
/// never prints it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSnapshot {
    pub harness: Option<String>,
    pub group: Option<String>,
    pub state: AgentState,
    /// When the state last changed, so presence can prefer the freshest agent
    /// between two in the same state. Filled with the injected `now`.
    pub updated_at: SystemTime,
}

/// Everything the presence needs, kept in one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    /// When the Krad ADE session started; Discord shows it as elapsed time.
    pub session_start: SystemTime,
    pub environment: Option<EnvironmentInfo>,
    pub groups: Vec<GroupInfo>,
    pub active_file: Option<PathBuf>,
    /// Whether the editor has the focus. Krad ADE sends `active_file: None`
    /// when focus moves elsewhere, such as an agent chat; the last file stays
    /// in [`Self::active_file`] so the presence does not flicker.
    pub editor_focused: bool,
    /// Only known from the workspace context: no event carries the cursor.
    pub cursor: Option<FilePosition>,
    /// Last file Krad ADE reported as saved.
    pub last_saved: Option<PathBuf>,
    /// Read by [`crate::git`] outside this module; presence shows it next to
    /// the project.
    pub branch: Option<String>,
    /// `origin` of the active project converted to `https`, when it has one.
    pub repo_url: Option<String>,
    /// Last time anything happened in the workspace, for the idle timeout.
    pub last_activity: SystemTime,
    /// Set by the `toggle` command; presence clears while paused.
    pub paused: bool,
    /// Agents seen through the app events, keyed by terminal id. Finished ones
    /// stay as [`AgentState::Idle`] and do not count as active.
    pub agents: BTreeMap<String, AgentSnapshot>,
}

impl Default for State {
    fn default() -> Self {
        Self::new(SystemTime::now())
    }
}

impl State {
    #[must_use]
    pub fn new(session_start: SystemTime) -> Self {
        Self {
            session_start,
            environment: None,
            groups: Vec::new(),
            active_file: None,
            editor_focused: false,
            cursor: None,
            last_saved: None,
            branch: None,
            repo_url: None,
            last_activity: session_start,
            paused: false,
            agents: BTreeMap::new(),
        }
    }

    /// Replaces the workspace part with the answer to `workspace/context`.
    ///
    /// Paused, the session start and the agents survive: the context is the
    /// workspace, not the extension's own state. `now` is injected so the
    /// module keeps no clock of its own; loading counts as activity.
    pub fn load(&mut self, context: &WorkspaceContextResult, now: SystemTime) {
        self.environment = context.environment.clone();
        self.groups = context.groups.clone();
        self.active_file = context.active_file.clone();
        self.editor_focused = context.active_file.is_some();
        self.cursor = context.cursor;
        self.last_activity = now;
    }

    /// Applies one workspace event. Pure: the caller injects `now`; nothing
    /// here reads the clock or the filesystem.
    pub fn apply(&mut self, event: &WorkspaceEvent, now: SystemTime) {
        self.last_activity = now;
        match event {
            WorkspaceEvent::EnvironmentOpened { environment } => {
                self.environment = Some(environment.clone());
            }
            WorkspaceEvent::EnvironmentClosed { environment } => {
                if self
                    .environment
                    .as_ref()
                    .is_some_and(|current| current.id == environment.id)
                {
                    self.environment = None;
                    self.active_file = None;
                    self.editor_focused = false;
                    self.cursor = None;
                }
            }
            WorkspaceEvent::GroupsChanged { groups } => {
                self.groups = groups.clone();
            }
            WorkspaceEvent::ActiveFileChanged { path } => {
                // Krad sends `None` whenever focus leaves the editor (a
                // terminal, another app). Keep the last file so the presence
                // does not flicker to idle; the idle timeout retires it.
                self.editor_focused = path.is_some();
                if let Some(path) = path {
                    self.active_file = Some(path.clone());
                }
                // The cursor of the previous file means nothing for this one.
                self.cursor = None;
            }
            WorkspaceEvent::FileSaved { path } => {
                self.last_saved = Some(path.clone());
            }
        }
    }

    /// Applies one app event. Pure: the caller injects `now` and nothing here
    /// reads the clock or the filesystem.
    ///
    /// Only agent events touch the map and count as activity. A finished agent
    /// stays as idle instead of being removed, so it stops counting as active
    /// without losing its harness or its group; events for a terminal that was
    /// never seen simply create an idle entry.
    pub fn apply_app_event(&mut self, params: &AppEventParams, now: SystemTime) {
        match &params.event {
            AppEvent::AgentStarted { terminal, harness } => {
                self.agents.insert(
                    terminal.clone(),
                    AgentSnapshot {
                        harness: harness.clone(),
                        group: params.group.clone(),
                        state: AgentState::Working,
                        updated_at: now,
                    },
                );
            }
            AppEvent::AgentWaiting { terminal } => {
                self.agent(terminal, params, now).state = AgentState::WaitingInput;
            }
            AppEvent::AgentFinished { terminal } => {
                self.agent(terminal, params, now).state = AgentState::Idle;
            }
            // Sessions, Git and the rest only matter for the refresh triggers
            // in `main`: they are not agent activity.
            _ => return,
        }
        self.last_activity = now;
    }

    /// The snapshot for `terminal`, created as idle if the start was missed.
    /// Keeps the harness and group already known for it.
    fn agent(
        &mut self,
        terminal: &str,
        params: &AppEventParams,
        now: SystemTime,
    ) -> &mut AgentSnapshot {
        let group = params.group.clone();
        self.agents
            .entry(terminal.to_owned())
            .and_modify(|agent| agent.updated_at = now)
            .or_insert_with(|| AgentSnapshot {
                harness: None,
                group,
                state: AgentState::Idle,
                updated_at: now,
            })
    }

    /// The group (project) that holds the active file.
    ///
    /// Uses the longest matching root, so nested groups pick the innermost one.
    /// Without an active file only a single group is unambiguous.
    #[must_use]
    pub fn active_group(&self) -> Option<&GroupInfo> {
        if let Some(file) = &self.active_file {
            return self
                .groups
                .iter()
                .filter(|group| file.starts_with(&group.root))
                .max_by_key(|group| group.root.components().count());
        }
        if self.groups.len() == 1 {
            return self.groups.first();
        }
        None
    }

    /// Records activity that does not arrive as a workspace event, such as a
    /// command from the palette.
    pub fn mark_activity(&mut self, now: SystemTime) {
        self.last_activity = now;
    }

    /// Flips pause and returns the new value.
    pub fn toggle_paused(&mut self) -> bool {
        self.paused = !self.paused;
        self.paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::protocol::HostInfo;
    use std::path::Path;
    use std::time::Duration;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn environment() -> EnvironmentInfo {
        EnvironmentInfo {
            id: "env-1".to_owned(),
            name: "Krad ADE".to_owned(),
            host: HostInfo::Local,
        }
    }

    fn group(id: &str, name: &str, root: &str) -> GroupInfo {
        GroupInfo {
            id: id.to_owned(),
            name: name.to_owned(),
            root: PathBuf::from(root),
        }
    }

    fn context() -> WorkspaceContextResult {
        WorkspaceContextResult {
            environment: Some(environment()),
            groups: vec![
                group("1", "api", "/code/api"),
                group("2", "api-web", "/code/api/web"),
            ],
            active_file: Some(PathBuf::from("/code/api/web/src/main.rs")),
            cursor: Some(FilePosition {
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
            &WorkspaceEvent::ActiveFileChanged {
                path: Some(PathBuf::from("/code/api/lib.rs")),
            },
            at(20),
        );
        assert_eq!(
            state.active_file.as_deref(),
            Some(Path::new("/code/api/lib.rs"))
        );
        assert_eq!(state.cursor, None, "a new file has an unknown cursor");
        assert!(state.editor_focused);
        assert_eq!(state.last_activity, at(20));

        state.apply(
            &WorkspaceEvent::FileSaved {
                path: PathBuf::from("/code/api/lib.rs"),
            },
            at(30),
        );
        assert_eq!(
            state.last_saved.as_deref(),
            Some(Path::new("/code/api/lib.rs"))
        );

        state.apply(
            &WorkspaceEvent::GroupsChanged {
                groups: vec![group("3", "other", "/code/other")],
            },
            at(40),
        );
        assert_eq!(state.groups, vec![group("3", "other", "/code/other")]);

        let before = state.active_file.clone();
        assert!(before.is_some());
        state.apply(&WorkspaceEvent::ActiveFileChanged { path: None }, at(50));
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

        state.apply(&WorkspaceEvent::ActiveFileChanged { path: None }, at(20));
        assert!(!state.editor_focused, "a chat or terminal took the focus");

        state.apply(
            &WorkspaceEvent::ActiveFileChanged {
                path: Some(PathBuf::from("/code/api/lib.rs")),
            },
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
        state.apply(
            &WorkspaceEvent::EnvironmentClosed { environment: other },
            at(20),
        );
        assert_eq!(state.environment, Some(environment()));
        assert!(state.active_file.is_some());

        state.apply(
            &WorkspaceEvent::EnvironmentClosed {
                environment: environment(),
            },
            at(30),
        );
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
        state.apply(
            &WorkspaceEvent::EnvironmentOpened {
                environment: opened.clone(),
            },
            at(10),
        );
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
            &WorkspaceEvent::ActiveFileChanged {
                path: Some(PathBuf::from("/code/api/src/main.rs")),
            },
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

    fn app_event(event: AppEvent) -> AppEventParams {
        AppEventParams {
            version: 1,
            environment: "env-1".to_owned(),
            group: Some("2".to_owned()),
            event,
        }
    }

    #[test]
    fn agent_events_fill_the_map_and_count_as_activity() {
        let mut state = State::new(SystemTime::UNIX_EPOCH);
        state.apply_app_event(
            &app_event(AppEvent::AgentStarted {
                terminal: "t-1".to_owned(),
                harness: Some("ClaudeCode".to_owned()),
            }),
            at(10),
        );
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

        state.apply_app_event(
            &app_event(AppEvent::AgentWaiting {
                terminal: "t-1".to_owned(),
            }),
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
        state.apply_app_event(
            &app_event(AppEvent::AgentStarted {
                terminal: "t-1".to_owned(),
                harness: Some("Codex".to_owned()),
            }),
            at(10),
        );
        state.apply_app_event(
            &app_event(AppEvent::AgentFinished {
                terminal: "t-1".to_owned(),
            }),
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
        state.apply_app_event(
            &app_event(AppEvent::AgentWaiting {
                terminal: "gone".to_owned(),
            }),
            at(10),
        );
        state.apply_app_event(
            &app_event(AppEvent::AgentFinished {
                terminal: "gone".to_owned(),
            }),
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

        state.apply_app_event(
            &app_event(AppEvent::SessionClosed {
                session: "unknown".to_owned(),
            }),
            at(30),
        );
        assert_eq!(
            state.last_activity,
            at(20),
            "sessions are not agent activity"
        );
    }

    #[test]
    fn non_agent_events_leave_the_agents_and_the_clock_alone() {
        let mut state = State::new(SystemTime::UNIX_EPOCH);
        state.apply_app_event(&app_event(AppEvent::GitChanged), at(10));
        assert!(state.agents.is_empty());
        assert_eq!(state.last_activity, SystemTime::UNIX_EPOCH);
    }
}
