//! Turns workspace state plus settings into what Discord should show.
//! Pure: the caller injects `now`; no app, IO or Discord here.

mod buttons;
mod icons;
mod text;

use crate::domain::agents::{AgentSnapshot, AgentState, AgentSummary};
use crate::domain::event::Group;
use crate::domain::settings::Settings;
use crate::domain::workspace::State;
use icons::{harness_icon, icon_url, language_icon, IDLE_IMAGE, LARGE_IMAGE};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use text::{agent_line, file_label, single_harness_text, truncate};

pub(crate) use buttons::PresenceButton;

const APP_NAME: &str = "Krad ADE";
const IDLE: &str = "Idle";

/// What to send to Discord, already detached from the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PresenceView {
    pub(crate) details: String,
    pub(crate) state: String,
    pub(crate) large_image: String,
    pub(crate) large_text: String,
    pub(crate) small_image: Option<String>,
    pub(crate) small_text: Option<String>,
    pub(crate) buttons: Vec<PresenceButton>,
    /// Unix seconds, the unit Discord expects.
    pub(crate) start_timestamp: i64,
}

/// The presence for this state and these settings at `now`, or `None` when it
/// must be hidden (paused, disabled, or inside an ignored group).
#[must_use]
pub(crate) fn view(state: &State, settings: &Settings, now: SystemTime) -> Option<PresenceView> {
    if state.paused || !settings.enabled {
        return None;
    }
    let group = state.active_group();
    if group.is_some_and(|group| settings.is_group_ignored(&group.name)) {
        return None;
    }

    let agents = agent_summary(state, settings);
    let idle = is_idle(state, settings, &agents, now);
    let (state_line, icon) = state_line(state, settings, &agents, idle);
    let small = small_icon(state, settings, &agents, idle, icon);

    Some(PresenceView {
        details: truncate(details(state, settings, group)),
        state: truncate(state_line),
        large_image: LARGE_IMAGE.to_owned(),
        large_text: truncate(large_text(state, settings)),
        small_image: small.image,
        small_text: small.text.map(truncate),
        buttons: buttons::list(state, settings),
        start_timestamp: unix_seconds(state.session_start),
    })
}

/// A working or waiting agent is activity: never flip to idle under it, even
/// when the last event is older than the timeout.
fn is_idle(state: &State, settings: &Settings, agents: &AgentSummary, now: SystemTime) -> bool {
    !agents.active()
        && now.duration_since(state.last_activity).unwrap_or_default()
            >= Duration::from_secs(u64::from(settings.idle_minutes) * 60)
}

/// The details line: the project (or the environment) and the Git branch.
fn details(state: &State, settings: &Settings, group: Option<&Group>) -> String {
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
    match (project, branch) {
        (Some(project), Some(branch)) => format!("{project} · {branch}"),
        (Some(project), None) => project,
        (None, Some(branch)) => format!("{APP_NAME} · {branch}"),
        (None, None) => APP_NAME.to_owned(),
    }
}

/// The large image tooltip: the app and the environment it runs in.
fn large_text(state: &State, settings: &Settings) -> String {
    if settings.show_project {
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
    }
}

/// The state line and the icon that goes with it.
fn state_line(
    state: &State,
    settings: &Settings,
    agents: &AgentSummary,
    idle: bool,
) -> (String, Option<(&'static str, &'static str)>) {
    if idle {
        (IDLE.to_owned(), Some((IDLE_IMAGE, IDLE)))
    } else if let Some(path) = state.active_file.as_deref().filter(|_| settings.show_file) {
        let mut line = file_label(path, state.cursor, settings.show_line);
        if settings.show_agents {
            if let Some(agents) = agent_line(agents) {
                line.push_str(" · ");
                line.push_str(&agents);
            }
        }
        (line, Some(language_icon(path)))
    } else if settings.show_agents {
        agent_line(agents).map_or_else(|| saved_or_idle(state, settings), |agents| (agents, None))
    } else {
        saved_or_idle(state, settings)
    }
}

/// The small image and its tooltip.
struct SmallIcon {
    image: Option<String>,
    text: Option<String>,
}

/// Away from the editor, the most urgent agent owns the small icon: that is
/// the one the user is probably talking to in a chat. Otherwise the file icon
/// stays, with the language or the lone harness as its tooltip.
fn small_icon(
    state: &State,
    settings: &Settings,
    agents: &AgentSummary,
    idle: bool,
    icon: Option<(&'static str, &'static str)>,
) -> SmallIcon {
    let chat_icon = if !state.editor_focused && settings.show_agents && !idle {
        agents.primary.as_ref().map(agent_icon)
    } else {
        None
    };
    let text = if let Some((_, text)) = &chat_icon {
        Some(text.clone())
    } else if settings.show_agents {
        single_harness_text(agents).or_else(|| icon.map(|(_, label)| label.to_owned()))
    } else {
        icon.map(|(_, label)| label.to_owned())
    };
    let image = chat_icon
        .map(|(asset, _)| icon_url(asset))
        .or_else(|| icon.map(|(asset, _)| icon_url(asset)));
    SmallIcon { image, text }
}

/// Counts the active agents, skipping the groups the user asked to ignore.
fn agent_summary(state: &State, settings: &Settings) -> AgentSummary {
    state
        .agents
        .summary(|id| group_is_ignored(state, settings, id))
}

/// The small icon and tooltip for the agent the presence is following.
fn agent_icon(agent: &AgentSnapshot) -> (&'static str, String) {
    let (asset, name) = harness_icon(agent.harness.as_deref());
    let doing = if agent.state == AgentState::WaitingInput {
        "waiting for input"
    } else {
        "working"
    };
    (asset, format!("{name} {doing}"))
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

/// Whether the group id carried by an agent event belongs to a group the user
/// asked to ignore: only the group list knows its name.
fn group_is_ignored(state: &State, settings: &Settings, id: Option<&str>) -> bool {
    let Some(id) = id else {
        return false;
    };
    state
        .groups
        .iter()
        .find(|group| group.id == id)
        .is_some_and(|group| settings.is_group_ignored(&group.name))
}

fn unix_seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
