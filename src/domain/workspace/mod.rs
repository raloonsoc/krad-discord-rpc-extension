//! What the extension knows about the workspace, and how events change it.
//! Pure: the startup context fills it once, events update it, `now` is given.

use crate::domain::agents::Agents;
use crate::domain::event::{CursorPosition, Environment, Event, Group};
use std::path::{Path, PathBuf};
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
    /// Id of the group the user was last active in. Krad ADE exposes no
    /// "active group", so this is a heuristic: the last group that saw
    /// activity from the user (a file, a save, an agent starting or asking for
    /// input, a session). It only counts while the group still exists.
    pub(crate) current_group: Option<String>,
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
            current_group: None,
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
        self.current_group = context
            .active_file
            .as_deref()
            .and_then(|file| self.group_holding(file))
            .map(|group| group.id.clone());
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
                    self.current_group = None;
                    self.editor_focused = false;
                    self.cursor = None;
                }
            }
            Event::GroupsChanged(groups) => {
                self.last_activity = now;
                self.groups = groups.clone();
                // The project the user was in may have been closed.
                if self
                    .current_group
                    .as_deref()
                    .is_some_and(|id| !groups.iter().any(|group| group.id == id))
                {
                    self.current_group = None;
                }
            }
            Event::FileFocused(path) => {
                self.last_activity = now;
                // Krad sends `None` whenever focus leaves the editor (a
                // terminal, another app). Keep the last file so the presence
                // does not flicker to idle; the idle timeout retires it. The
                // file also says which project the user is in.
                self.editor_focused = path.is_some();
                if let Some(path) = path {
                    self.current_group = self.group_holding(path).map(|group| group.id.clone());
                    self.active_file = Some(path.clone());
                }
                // The cursor of the previous file means nothing for this one.
                self.cursor = None;
            }
            Event::FileSaved(path) => {
                self.last_activity = now;
                self.current_group = self.group_holding(path).map(|group| group.id.clone());
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
                // Starting an agent almost always means the user is in its
                // chat: follow that group.
                if let Some(group) = group {
                    self.current_group = Some(group.clone());
                }
            }
            Event::AgentWaiting { terminal, group } => {
                self.agents.waiting(terminal, group.clone(), now);
                self.last_activity = now;
                // An agent asking for input brings the user to its chat.
                if let Some(group) = group {
                    self.current_group = Some(group.clone());
                }
            }
            Event::AgentFinished { terminal, group } => {
                // A background agent ending says nothing about where the user
                // is, so the project does not move.
                self.agents.finished(terminal, group.clone(), now);
                self.last_activity = now;
            }
            // A session opening is a chat the user just entered, so it moves
            // the project like an agent does. Sessions and Git changes are not
            // workspace or agent activity.
            Event::SessionOpened { group } => {
                if let Some(group) = group {
                    self.current_group = Some(group.clone());
                }
            }
            Event::GitChanged | Event::SessionClosed => {}
        }
    }

    /// The group (project) the user is in.
    ///
    /// Prefers [`Self::current_group`], the last group with user activity,
    /// because Krad ADE reports no active group. Falls back to the group that
    /// holds the active file, then to a single group. Uses the longest
    /// matching root, so nested groups pick the innermost one.
    #[must_use]
    pub(crate) fn active_group(&self) -> Option<&Group> {
        if let Some(group) = self
            .current_group
            .as_ref()
            .and_then(|id| self.groups.iter().find(|group| &group.id == id))
        {
            return Some(group);
        }
        if let Some(file) = &self.active_file {
            return self.group_holding(file);
        }
        if self.groups.len() == 1 {
            return self.groups.first();
        }
        None
    }

    /// The innermost group whose root contains `path`.
    fn group_holding(&self, path: &Path) -> Option<&Group> {
        self.groups
            .iter()
            .filter(|group| path.starts_with(&group.root))
            .max_by_key(|group| group.root.components().count())
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
