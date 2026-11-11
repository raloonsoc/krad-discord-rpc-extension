//! The links Discord shows under the activity, within its limits.

use crate::domain::settings::Settings;
use crate::domain::workspace::State;

/// Discord accepts at most two buttons per activity.
pub(crate) const MAX_BUTTONS: usize = 2;
pub(crate) const MAX_BUTTON_LABEL: usize = 32;
pub(crate) const MAX_BUTTON_URL: usize = 512;

pub(crate) const KRAD_BUTTON_LABEL: &str = "Get Krad ADE";
pub(crate) const KRAD_BUTTON_URL: &str = "https://github.com/joordih/krad-ade";
pub(crate) const REPO_BUTTON_LABEL: &str = "View repository";

/// One of the links Discord shows under the activity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PresenceButton {
    pub(crate) label: String,
    pub(crate) url: String,
}

/// Fixed links Discord shows below the activity. Discord caps them at two and
/// silently rejects longer labels or URLs, so anything invalid is dropped here
/// instead of losing the whole update.
pub(crate) fn list(state: &State, settings: &Settings) -> Vec<PresenceButton> {
    let mut buttons = Vec::new();
    if settings.show_krad_button {
        if let Some(button) = button(KRAD_BUTTON_LABEL, KRAD_BUTTON_URL) {
            buttons.push(button);
        }
    }
    if settings.show_project && settings.show_repo_button {
        if let Some(button) = state
            .repo_url
            .as_deref()
            .and_then(|url| button(REPO_BUTTON_LABEL, url))
        {
            buttons.push(button);
        }
    }
    buttons.truncate(MAX_BUTTONS);
    buttons
}

fn button(label: &str, url: &str) -> Option<PresenceButton> {
    let label = label.trim();
    let url = url.trim();
    if label.is_empty()
        || url.is_empty()
        || label.chars().count() > MAX_BUTTON_LABEL
        || url.chars().count() > MAX_BUTTON_URL
        || !url.starts_with("https://")
    {
        return None;
    }
    Some(PresenceButton {
        label: label.to_owned(),
        url: url.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_that_break_discord_limits_are_dropped() {
        let long_label = "x".repeat(MAX_BUTTON_LABEL + 1);
        let long_url = format!("https://example.com/{}", "x".repeat(MAX_BUTTON_URL));
        assert_eq!(button(&long_label, KRAD_BUTTON_URL), None);
        assert_eq!(button(KRAD_BUTTON_LABEL, &long_url), None);
        assert_eq!(button(KRAD_BUTTON_LABEL, "http://example.com"), None);
        assert_eq!(button("", KRAD_BUTTON_URL), None);
        assert!(button(KRAD_BUTTON_LABEL, KRAD_BUTTON_URL).is_some());
    }
}
