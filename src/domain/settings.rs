//! The settings the extension contributes, with their defaults and ranges.

/// Application ID of the official "Krad ADE" Discord app. It is public, not a
/// secret. The `client-id` setting overrides it; left empty, this one is used.
pub(crate) const DEFAULT_CLIENT_ID: &str = "1569775639412613150";

/// Minutes without activity before the presence turns idle.
pub(crate) const DEFAULT_IDLE_MINUTES: u32 = 5;
pub(crate) const MIN_IDLE_MINUTES: u32 = 1;
pub(crate) const MAX_IDLE_MINUTES: u32 = 120;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) client_id: String,
    pub(crate) show_project: bool,
    pub(crate) show_file: bool,
    pub(crate) show_line: bool,
    pub(crate) show_branch: bool,
    /// Show what the coding agents are doing. Does not affect their ability to
    /// keep the presence awake.
    pub(crate) show_agents: bool,
    pub(crate) idle_minutes: u32,
    pub(crate) show_krad_button: bool,
    pub(crate) show_repo_button: bool,
    /// Lowercased, trimmed group names. Empty entries are dropped.
    pub(crate) ignored_groups: Vec<String>,
    pub(crate) enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_id: DEFAULT_CLIENT_ID.to_owned(),
            show_project: true,
            show_file: true,
            show_line: false,
            show_branch: true,
            show_agents: true,
            idle_minutes: DEFAULT_IDLE_MINUTES,
            show_krad_button: true,
            show_repo_button: false,
            ignored_groups: Vec::new(),
            enabled: true,
        }
    }
}

impl Settings {
    /// Trims the id and falls back to the official app when it is empty.
    pub(crate) fn set_client_id(&mut self, raw: &str) {
        let id = raw.trim();
        id.clone_into(&mut self.client_id);
        if id.is_empty() {
            DEFAULT_CLIENT_ID.clone_into(&mut self.client_id);
        }
    }

    pub(crate) fn set_idle_minutes(&mut self, minutes: f64) {
        self.idle_minutes = clamp_idle_minutes(minutes);
    }

    /// Parses the comma-separated list into lowercased, trimmed names.
    pub(crate) fn set_ignored_groups(&mut self, raw: &str) {
        self.ignored_groups = parse_ignored_groups(raw);
    }

    /// Whether the presence must stay hidden for this group.
    #[must_use]
    pub(crate) fn is_group_ignored(&self, name: &str) -> bool {
        self.ignored_groups
            .iter()
            .any(|ignored| ignored.eq_ignore_ascii_case(name.trim()))
    }
}

/// Discord numbers arrive as `f64`; zero is not a valid idle timeout, so it
/// (and any other out-of-range value) is clamped instead of disabling idle.
fn clamp_idle_minutes(minutes: f64) -> u32 {
    if !minutes.is_finite() {
        return DEFAULT_IDLE_MINUTES;
    }
    let minutes = minutes.round();
    if minutes < f64::from(MIN_IDLE_MINUTES) {
        MIN_IDLE_MINUTES
    } else if minutes > f64::from(MAX_IDLE_MINUTES) {
        MAX_IDLE_MINUTES
    } else {
        minutes as u32
    }
}

fn parse_ignored_groups(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_minutes_are_clamped_into_a_sane_range() {
        let cases = [
            (0.0, 1),
            (1.0, 1),
            (5.4, 5),
            (5.6, 6),
            (120.0, 120),
            (500.0, 120),
        ];
        for (input, expected) in cases {
            assert_eq!(clamp_idle_minutes(input), expected, "{input}");
        }
    }

    #[test]
    fn empty_ignored_groups_parse_to_nothing() {
        assert!(parse_ignored_groups(" , ,, ").is_empty());
        assert_eq!(parse_ignored_groups("a,B"), ["a", "b"]);
    }
}
