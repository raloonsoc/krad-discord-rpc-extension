//! What the extension knows about the workspace, and how events change it.
//! Pure: the startup context fills it once, events update it, `now` is given.

use crate::domain::agents::Agents;
use crate::domain::event::{CursorPosition, Environment, Event, Group};
use std::path::PathBuf;
use std::time::SystemTime;

/// The workspace as Krad ADE reports it at startup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceContext {
    pub(crate) environment: Option<Environment>,
    pub(crate) groups: Vec<Group>,
    pub(crate) active_file: Option<PathBuf>,
    pub(crate) cursor: Option<CursorPosition>,
}

/// Everything the presence needs, kept in one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct State {
    /// When the Krad ADE session started; Discord shows it as elapsed time.
    pub(crate) session_start: SystemTime,
    pub(crate) environment: Option<Environment>,
    pub(crate) groups: Vec<Group>,
    pub(crate) active_file: Option<PathBuf>,
    /// Whether the editor has the focus. Krad ADE sends no active file when
    /// focus moves elsewhere, such as an agent chat; the last file stays in
    /// [`Self::active_file`] so the presence does not flicker.
    pub(crate) editor_focused: bool,
    /// Only known from the workspace context: no event carries the cursor.
    pub(crate) cursor: Option<CursorPosition>,
    /// Last file Krad ADE reported as saved.
    pub(crate) last_saved: Option<PathBuf>,
    /// Read by the Git adapter; presence shows it next to the project.
    pub(crate) branch: Option<String>,
    /// `origin` of the active project converted to `https`, when it has one.
    pub(crate) repo_url: Option<String>,
    /// Last time anything happened in the workspace, for the idle timeout.
    pub(crate) last_activity: SystemTime,
    /// Set by the `toggle` command; presence clears while paused.
    pub(crate) paused: bool,
    /// Agents seen through the app events, keyed by terminal id. Finished ones
    /// stay as [`AgentState::Idle`] and do not count as active.
    pub(crate) agents: Agents,
}

impl State {
    #[must_use]
    pub(crate) fn new(session_start: SystemTime) -> Self {
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
            agents: Agents::default(),
        }
    }

    /// Replaces the workspace part with the startup context.
    ///
    /// Paused, the session start and the agents survive: the context is the
    /// workspace, not the extension's own state. `now` is injected so the
    /// module keeps no clock of its own; loading counts as activity.
    pub(crate) fn load(&mut self, context: &WorkspaceContext, now: SystemTime) {
        self.environment = context.environment.clone();
        self.groups = context.groups.clone();
        self.active_file = context.active_file.clone();
        self.editor_focused = context.active_file.is_some();
        self.cursor = context.cursor;
        self.last_activity = now;
    }

    /// Applies one event. Pure: the caller injects `now`; nothing here reads
    /// the clock or the filesystem.
    ///
    /// Only workspace and agent events count as activity. A finished agent
    /// stays as idle instead of being removed, so it stops counting as active
    /// without losing its harness or its group; events for a terminal that was
    /// never seen simply create an idle entry.
    pub(crate) fn apply(&mut self, event: &Event, now: SystemTime) {
        match event {
            Event::EnvironmentOpened(environment) => {
                self.last_activity = now;
                self.environment = Some(environment.clone());
            }
            Event::EnvironmentClosed(environment) => {
                self.last_activity = now;
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
            Event::GroupsChanged(groups) => {
                self.last_activity = now;
                self.groups = groups.clone();
            }
            Event::FileFocused(path) => {
                self.last_activity = now;
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
            Event::FileSaved(path) => {
                self.last_activity = now;
                self.last_saved = Some(path.clone());
            }
            Event::AgentStarted {
                terminal,
                harness,
                group,
            } => {
                self.agents
                    .started(terminal, harness.clone(), group.clone(), now);
                self.last_activity = now;
            }
            Event::AgentWaiting { terminal, group } => {
                self.agents.waiting(terminal, group.clone(), now);
                self.last_activity = now;
            }
            Event::AgentFinished { terminal, group } => {
                self.agents.finished(terminal, group.clone(), now);
                self.last_activity = now;
            }
            // Sessions and Git changes only matter for the refresh triggers in
            // the app: they are not workspace or agent activity.
            Event::GitChanged | Event::SessionOpened | Event::SessionClosed => {}
        }
    }

    /// The group (project) that holds the active file.
    ///
    /// Uses the longest matching root, so nested groups pick the innermost one.
    /// Without an active file only a single group is unambiguous.
    #[must_use]
    pub(crate) fn active_group(&self) -> Option<&Group> {
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

    /// Records activity that does not arrive as an event, such as a command
    /// from the palette.
    pub(crate) fn mark_activity(&mut self, now: SystemTime) {
        self.last_activity = now;
    }

    /// Flips pause and returns the new value.
    pub(crate) fn toggle_paused(&mut self) -> bool {
        self.paused = !self.paused;
        self.paused
    }
}

#[cfg(test)]
mod tests;
