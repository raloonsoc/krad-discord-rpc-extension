//! The events the extension reacts to, in its own vocabulary.

use std::path::PathBuf;

/// A workspace group (project): the unit the presence calls a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Group {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) root: PathBuf,
}

/// The environment Krad ADE runs in, as far as the presence shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Environment {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// A cursor position inside the active file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CursorPosition {
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// One thing that happened, already detached from the host SDK.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    EnvironmentOpened(Environment),
    EnvironmentClosed(Environment),
    GroupsChanged(Vec<Group>),
    /// `None` means the editor lost the focus; the caller keeps the last file.
    FileFocused(Option<PathBuf>),
    FileSaved(PathBuf),
    AgentStarted {
        terminal: String,
        harness: Option<String>,
        group: Option<String>,
    },
    AgentWaiting {
        terminal: String,
        group: Option<String>,
    },
    AgentFinished {
        terminal: String,
        group: Option<String>,
    },
    GitChanged,
    /// `group` is the group id carried by the app event, used to follow the
    /// chat the user just opened.
    SessionOpened {
        group: Option<String>,
    },
    SessionClosed,
}

impl Event {
    /// Workspace, agent and session events can move the active group, and
    /// with it the branch or remote, so they wake the Git monitor. A finished
    /// agent and a closed session cannot: agents in the background end on
    /// their own and must not change the project.
    #[must_use]
    pub(crate) fn wants_git_refresh(&self) -> bool {
        !matches!(self, Self::AgentFinished { .. } | Self::SessionClosed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_grouped(event: fn(String, Option<String>) -> Event) -> Event {
        event("t-1".to_owned(), Some("1".to_owned()))
    }

    #[test]
    fn events_that_can_move_the_project_wake_the_git_monitor() {
        assert!(Event::FileFocused(None).wants_git_refresh());
        assert!(
            Event::FileFocused(Some(PathBuf::from("/code/api/src/main.rs"))).wants_git_refresh()
        );
        assert!(Event::FileSaved(PathBuf::from("/code/api/src/main.rs")).wants_git_refresh());
        assert!(Event::GroupsChanged(Vec::new()).wants_git_refresh());
        assert!(Event::GitChanged.wants_git_refresh());
        assert!(Event::SessionOpened {
            group: Some("1".to_owned()),
        }
        .wants_git_refresh());
        assert!(
            agent_grouped(|terminal, group| Event::AgentStarted {
                terminal,
                harness: None,
                group,
            })
            .wants_git_refresh(),
            "an agent starting moves the project to its chat"
        );
        assert!(
            agent_grouped(|terminal, group| Event::AgentWaiting { terminal, group })
                .wants_git_refresh(),
            "an agent asking for input moves the project to its chat"
        );
    }

    #[test]
    fn background_ends_never_wake_the_git_monitor() {
        assert!(
            !agent_grouped(|terminal, group| Event::AgentFinished { terminal, group })
                .wants_git_refresh()
        );
        assert!(!Event::SessionClosed.wants_git_refresh());
    }
}
