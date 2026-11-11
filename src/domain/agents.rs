//! The agents seen through app events, reduced to what the presence shows.

use std::collections::BTreeMap;
use std::time::SystemTime;

/// What an agent is doing, as far as the presence cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentState {
    /// Finished, or not known to be doing anything. Not active.
    Idle,
    Working,
    WaitingInput,
}

/// One agent, keyed by its terminal, reduced to what Rich Presence shows.
///
/// `group` is the group id carried by the app event, not its name: presence
/// resolves it against the group list only to honour `ignored-groups` and
/// never prints it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentSnapshot {
    pub(crate) harness: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) state: AgentState,
    /// When the state last changed, so presence can prefer the freshest agent
    /// between two in the same state. Filled with the injected `now`.
    pub(crate) updated_at: SystemTime,
}

/// Every agent Krad ADE has announced, keyed by terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Agents {
    entries: BTreeMap<String, AgentSnapshot>,
}

impl Agents {
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A start always replaces what was known for that terminal.
    pub(crate) fn started(
        &mut self,
        terminal: &str,
        harness: Option<String>,
        group: Option<String>,
        now: SystemTime,
    ) {
        self.entries.insert(
            terminal.to_owned(),
            AgentSnapshot {
                harness,
                group,
                state: AgentState::Working,
                updated_at: now,
            },
        );
    }

    pub(crate) fn waiting(&mut self, terminal: &str, group: Option<String>, now: SystemTime) {
        self.touch(terminal, group, now).state = AgentState::WaitingInput;
    }

    pub(crate) fn finished(&mut self, terminal: &str, group: Option<String>, now: SystemTime) {
        self.touch(terminal, group, now).state = AgentState::Idle;
    }

    /// The snapshot for `terminal`, created as idle if the start was missed.
    /// Keeps the harness and group already known for it.
    fn touch(
        &mut self,
        terminal: &str,
        group: Option<String>,
        now: SystemTime,
    ) -> &mut AgentSnapshot {
        self.entries
            .entry(terminal.to_owned())
            .and_modify(|agent| agent.updated_at = now)
            .or_insert_with(|| AgentSnapshot {
                harness: None,
                group,
                state: AgentState::Idle,
                updated_at: now,
            })
    }

    /// Counts what is working or waiting, skipping the groups `ignored` hides.
    /// Finished agents stay in the map but are not active.
    pub(crate) fn summary(&self, ignored: impl Fn(Option<&str>) -> bool) -> AgentSummary {
        let mut summary = AgentSummary::default();
        let mut harnesses = Vec::new();
        for agent in self.entries.values() {
            if ignored(agent.group.as_deref()) {
                continue;
            }
            match agent.state {
                AgentState::Idle => continue,
                AgentState::Working => summary.working += 1,
                AgentState::WaitingInput => summary.waiting += 1,
            }
            harnesses.push(agent.harness.as_deref());
            let replace = summary
                .primary
                .as_ref()
                .is_none_or(|current| more_urgent(agent, current));
            if replace {
                summary.primary = Some(agent.clone());
            }
        }
        if harnesses.len() == 1 {
            summary.only_harness = harnesses[0].map(str::to_owned);
        }
        summary
    }

    #[cfg(test)]
    pub(crate) fn get(&self, terminal: &str) -> Option<&AgentSnapshot> {
        self.entries.get(terminal)
    }

    #[cfg(test)]
    pub(crate) fn get_mut(&mut self, terminal: &str) -> Option<&mut AgentSnapshot> {
        self.entries.get_mut(terminal)
    }

    #[cfg(test)]
    pub(crate) fn insert(&mut self, terminal: String, agent: AgentSnapshot) {
        self.entries.insert(terminal, agent);
    }
}

/// What the active agents add to the presence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AgentSummary {
    pub(crate) working: usize,
    pub(crate) waiting: usize,
    /// Harness of the only active agent, when there is exactly one.
    pub(crate) only_harness: Option<String>,
    /// The agent worth showing on its own: waiting beats working, then the
    /// most recently updated. `None` while nothing is active.
    pub(crate) primary: Option<AgentSnapshot>,
}

impl AgentSummary {
    #[must_use]
    pub(crate) fn active(&self) -> bool {
        self.count() > 0
    }

    #[must_use]
    pub(crate) fn count(&self) -> usize {
        self.working + self.waiting
    }
}

/// Waiting beats working; between two in the same state the freshest wins.
fn more_urgent(candidate: &AgentSnapshot, current: &AgentSnapshot) -> bool {
    (urgency(candidate.state), candidate.updated_at) > (urgency(current.state), current.updated_at)
}

fn urgency(state: AgentState) -> u8 {
    match state {
        AgentState::Idle => 0,
        AgentState::Working => 1,
        AgentState::WaitingInput => 2,
    }
}
