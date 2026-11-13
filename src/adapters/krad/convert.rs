//! Translates host SDK types into the extension's own domain types.

use crate::domain::event::{CursorPosition, Environment, Event, Group};
use crate::domain::workspace::WorkspaceContext;
use krad_extension_sdk::api::protocol::{
    AppEvent, AppEventParams, EnvironmentInfo, FilePosition, GroupInfo, WorkspaceContextResult,
    WorkspaceEvent,
};

/// The startup context, translated to the domain.
#[must_use]
pub(crate) fn workspace_context(context: &WorkspaceContextResult) -> WorkspaceContext {
    WorkspaceContext {
        environment: context.environment.as_ref().map(environment),
        groups: context.groups.iter().map(group).collect(),
        active_file: context.active_file.clone(),
        cursor: context.cursor.map(cursor),
    }
}

fn environment(info: &EnvironmentInfo) -> Environment {
    Environment {
        id: info.id.clone(),
        name: info.name.clone(),
    }
}

fn group(info: &GroupInfo) -> Group {
    Group {
        id: info.id.clone(),
        name: info.name.clone(),
        root: info.root.clone(),
    }
}

fn cursor(position: FilePosition) -> CursorPosition {
    CursorPosition {
        line: position.line,
        column: position.column,
    }
}

#[must_use]
pub(crate) fn workspace_event(event: &WorkspaceEvent) -> Event {
    match event {
        WorkspaceEvent::EnvironmentOpened { environment: info } => {
            Event::EnvironmentOpened(environment(info))
        }
        WorkspaceEvent::EnvironmentClosed { environment: info } => {
            Event::EnvironmentClosed(environment(info))
        }
        WorkspaceEvent::GroupsChanged { groups } => {
            Event::GroupsChanged(groups.iter().map(group).collect())
        }
        WorkspaceEvent::ActiveFileChanged { path } => Event::FileFocused(path.clone()),
        WorkspaceEvent::FileSaved { path } => Event::FileSaved(path.clone()),
    }
}

/// App events the presence cares about; the rest are only logged.
#[must_use]
pub(crate) fn app_event(params: &AppEventParams) -> Option<Event> {
    Some(match &params.event {
        AppEvent::AgentStarted { terminal, harness } => Event::AgentStarted {
            terminal: terminal.clone(),
            harness: harness.clone(),
            group: params.group.clone(),
        },
        AppEvent::AgentWaiting { terminal } => Event::AgentWaiting {
            terminal: terminal.clone(),
            group: params.group.clone(),
        },
        AppEvent::AgentFinished { terminal } => Event::AgentFinished {
            terminal: terminal.clone(),
            group: params.group.clone(),
        },
        AppEvent::GitChanged => Event::GitChanged,
        AppEvent::SessionOpened { .. } => Event::SessionOpened {
            group: params.group.clone(),
        },
        AppEvent::SessionClosed { .. } => Event::SessionClosed,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::protocol::HostInfo;
    use std::path::{Path, PathBuf};

    fn environment_info() -> EnvironmentInfo {
        EnvironmentInfo {
            id: "env-1".to_owned(),
            name: "Krad ADE".to_owned(),
            host: HostInfo::Local,
        }
    }

    fn group_info() -> GroupInfo {
        GroupInfo {
            id: "1".to_owned(),
            name: "api".to_owned(),
            root: PathBuf::from("/code/api"),
        }
    }

    fn context() -> WorkspaceContextResult {
        WorkspaceContextResult {
            environment: Some(environment_info()),
            groups: vec![group_info()],
            active_file: Some(PathBuf::from("/code/api/src/main.rs")),
            cursor: Some(FilePosition {
                line: 42,
                column: 7,
            }),
        }
    }

    fn params(event: AppEvent) -> AppEventParams {
        AppEventParams {
            version: 1,
            environment: "env-1".to_owned(),
            group: Some("1".to_owned()),
            event,
        }
    }

    #[test]
    fn host_types_become_domain_types() {
        let context = workspace_context(&context());
        assert_eq!(
            context.environment,
            Some(Environment {
                id: "env-1".to_owned(),
                name: "Krad ADE".to_owned(),
            })
        );
        assert_eq!(context.groups.len(), 1);
        assert_eq!(
            context.active_file.as_deref(),
            Some(Path::new("/code/api/src/main.rs"))
        );
        assert_eq!(
            context.cursor,
            Some(CursorPosition {
                line: 42,
                column: 7
            })
        );

        assert_eq!(
            workspace_event(&WorkspaceEvent::ActiveFileChanged { path: None }),
            Event::FileFocused(None)
        );
        assert_eq!(
            workspace_event(&WorkspaceEvent::FileSaved {
                path: PathBuf::from("/code/api/src/lib.rs"),
            }),
            Event::FileSaved(PathBuf::from("/code/api/src/lib.rs"))
        );
        assert_eq!(
            workspace_event(&WorkspaceEvent::GroupsChanged {
                groups: vec![group_info()],
            }),
            Event::GroupsChanged(vec![Group {
                id: "1".to_owned(),
                name: "api".to_owned(),
                root: PathBuf::from("/code/api"),
            }])
        );

        assert_eq!(
            app_event(&params(AppEvent::AgentStarted {
                terminal: "t-1".to_owned(),
                harness: Some("Codex".to_owned()),
            })),
            Some(Event::AgentStarted {
                terminal: "t-1".to_owned(),
                harness: Some("Codex".to_owned()),
                group: Some("1".to_owned()),
            })
        );
        assert_eq!(
            app_event(&params(AppEvent::AgentWaiting {
                terminal: "t-1".to_owned(),
            })),
            Some(Event::AgentWaiting {
                terminal: "t-1".to_owned(),
                group: Some("1".to_owned()),
            })
        );
        assert_eq!(
            app_event(&params(AppEvent::AgentFinished {
                terminal: "t-1".to_owned(),
            })),
            Some(Event::AgentFinished {
                terminal: "t-1".to_owned(),
                group: Some("1".to_owned()),
            })
        );
        assert_eq!(
            app_event(&params(AppEvent::GitChanged)),
            Some(Event::GitChanged)
        );
        assert_eq!(
            app_event(&params(AppEvent::SessionOpened {
                session: "s-1".to_owned(),
            })),
            Some(Event::SessionOpened {
                group: Some("1".to_owned()),
            })
        );
        assert_eq!(
            app_event(&params(AppEvent::SessionClosed {
                session: "s-1".to_owned(),
            })),
            Some(Event::SessionClosed)
        );
        assert_eq!(app_event(&params(AppEvent::WorktreeRemoved)), None);
    }
}
