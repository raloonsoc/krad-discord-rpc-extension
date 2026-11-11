//! Reads the host's `SettingsValues` into the domain [`Settings`].

use crate::domain::settings::Settings;
use krad_extension_sdk::api::protocol::{SettingValue, SettingsValues};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    ClientId,
    ShowProject,
    ShowFile,
    ShowLine,
    ShowBranch,
    ShowAgents,
    IdleMinutes,
    ShowKradButton,
    ShowRepoButton,
    IgnoredGroups,
    Enabled,
}

impl Key {
    const ALL: [Self; 11] = [
        Self::ClientId,
        Self::ShowProject,
        Self::ShowFile,
        Self::ShowLine,
        Self::ShowBranch,
        Self::ShowAgents,
        Self::IdleMinutes,
        Self::ShowKradButton,
        Self::ShowRepoButton,
        Self::IgnoredGroups,
        Self::Enabled,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::ClientId => "client-id",
            Self::ShowProject => "show-project",
            Self::ShowFile => "show-file",
            Self::ShowLine => "show-line",
            Self::ShowBranch => "show-branch",
            Self::ShowAgents => "show-agents",
            Self::IdleMinutes => "idle-minutes",
            Self::ShowKradButton => "show-krad-button",
            Self::ShowRepoButton => "show-repo-button",
            Self::IgnoredGroups => "ignored-groups",
            Self::Enabled => "enabled",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.name() == name)
    }
}

#[must_use]
pub(crate) fn settings_from_values(values: &SettingsValues) -> Settings {
    let mut settings = Settings::default();
    for (name, value) in values {
        if let Some(key) = Key::from_name(name.as_ref()) {
            apply(&mut settings, key, value);
        }
    }
    settings
}

fn apply(settings: &mut Settings, key: Key, value: &SettingValue) {
    match (key, value) {
        (Key::ClientId, SettingValue::Text(id)) => settings.set_client_id(id),
        (Key::ShowProject, SettingValue::Toggle(show)) => settings.show_project = *show,
        (Key::ShowFile, SettingValue::Toggle(show)) => settings.show_file = *show,
        (Key::ShowLine, SettingValue::Toggle(show)) => settings.show_line = *show,
        (Key::ShowBranch, SettingValue::Toggle(show)) => settings.show_branch = *show,
        (Key::ShowAgents, SettingValue::Toggle(show)) => settings.show_agents = *show,
        (Key::IdleMinutes, SettingValue::Number(minutes)) => settings.set_idle_minutes(*minutes),
        (Key::ShowKradButton, SettingValue::Toggle(show)) => settings.show_krad_button = *show,
        (Key::ShowRepoButton, SettingValue::Toggle(show)) => settings.show_repo_button = *show,
        (Key::IgnoredGroups, SettingValue::Text(names)) => settings.set_ignored_groups(names),
        (Key::Enabled, SettingValue::Toggle(enabled)) => settings.enabled = *enabled,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::settings::DEFAULT_CLIENT_ID;
    use krad_extension_sdk::api::SettingKey;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn values(
        entries: &[(&str, SettingValue)],
    ) -> Result<SettingsValues, Box<dyn std::error::Error>> {
        entries
            .iter()
            .map(|(name, value)| Ok((name.parse::<SettingKey>()?, value.clone())))
            .collect()
    }

    #[test]
    fn no_settings_keep_the_defaults() {
        assert_eq!(
            settings_from_values(&SettingsValues::new()),
            Settings::default()
        );
        assert_eq!(Settings::default().client_id, DEFAULT_CLIENT_ID);
        assert!(!DEFAULT_CLIENT_ID.is_empty());
        assert!(Settings::default().show_project);
        assert!(Settings::default().show_file);
        assert!(!Settings::default().show_line);
        assert!(Settings::default().show_branch);
        assert!(Settings::default().show_agents);
        assert_eq!(Settings::default().idle_minutes, 5);
        assert!(Settings::default().show_krad_button);
        assert!(!Settings::default().show_repo_button);
        assert!(Settings::default().enabled);
    }

    #[test]
    fn settings_are_read_from_the_host_values() -> TestResult {
        let settings = settings_from_values(&values(&[
            ("client-id", SettingValue::Text(" 1234567890 ".to_owned())),
            ("show-project", SettingValue::Toggle(false)),
            ("show-file", SettingValue::Toggle(false)),
            ("show-line", SettingValue::Toggle(true)),
            ("show-branch", SettingValue::Toggle(false)),
            ("show-agents", SettingValue::Toggle(false)),
            ("idle-minutes", SettingValue::Number(15.0)),
            ("show-krad-button", SettingValue::Toggle(false)),
            ("show-repo-button", SettingValue::Toggle(true)),
            (
                "ignored-groups",
                SettingValue::Text(" client-a, ,Secret ".to_owned()),
            ),
            ("enabled", SettingValue::Toggle(false)),
        ])?);
        assert_eq!(settings.client_id, "1234567890");
        assert!(!settings.show_project);
        assert!(!settings.show_file);
        assert!(settings.show_line);
        assert!(!settings.show_branch);
        assert!(!settings.show_agents);
        assert_eq!(settings.idle_minutes, 15);
        assert!(!settings.show_krad_button);
        assert!(settings.show_repo_button);
        assert_eq!(settings.ignored_groups, ["client-a", "secret"]);
        assert!(!settings.enabled);
        Ok(())
    }

    #[test]
    fn an_empty_client_id_falls_back_to_the_official_app() -> TestResult {
        let settings = settings_from_values(&values(&[(
            "client-id",
            SettingValue::Text("  ".to_owned()),
        )])?);
        assert_eq!(settings.client_id, DEFAULT_CLIENT_ID);
        Ok(())
    }

    #[test]
    fn ignored_groups_match_names_ignoring_case_and_spaces() -> TestResult {
        let settings = settings_from_values(&values(&[(
            "ignored-groups",
            SettingValue::Text("Client-A, API".to_owned()),
        )])?);
        assert!(settings.is_group_ignored("client-a"));
        assert!(settings.is_group_ignored("  CLIENT-A "));
        assert!(settings.is_group_ignored("api"));
        assert!(!settings.is_group_ignored("client"));
        assert!(!settings.is_group_ignored("api-web"));
        Ok(())
    }

    #[test]
    fn mistyped_values_keep_the_defaults() -> TestResult {
        let settings = settings_from_values(&values(&[
            ("client-id", SettingValue::Toggle(true)),
            ("show-project", SettingValue::Text("yes".to_owned())),
            ("show-file", SettingValue::Number(1.0)),
            ("show-branch", SettingValue::Number(1.0)),
            ("show-agents", SettingValue::Text("yes".to_owned())),
            ("idle-minutes", SettingValue::Text("ten".to_owned())),
            ("show-krad-button", SettingValue::Number(1.0)),
            ("show-repo-button", SettingValue::Text("yes".to_owned())),
            ("enabled", SettingValue::Number(0.0)),
            ("unknown", SettingValue::Toggle(false)),
        ])?);
        assert_eq!(settings, Settings::default());
        Ok(())
    }
}
