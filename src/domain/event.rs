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
    SessionOpened,
    SessionClosed,
}

impl Event {
    /// Workspace and Git events can change the branch or remote; agent and
    /// session events cannot, so they do not wake the Git monitor.
    #[must_use]
    pub(crate) fn wants_git_refresh(&self) -> bool {
        !matches!(
            self,
            Self::AgentStarted { .. }
                | Self::AgentWaiting { .. }
                | Self::AgentFinished { .. }
                | Self::SessionOpened
                | Self::SessionClosed
        )
    }
}
