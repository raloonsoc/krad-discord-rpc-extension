//! Maps a presence view to the Discord activity payload.

use crate::domain::presence::PresenceView;
use discord_rich_presence::activity;

pub(crate) fn to_activity(view: &PresenceView) -> activity::Activity<'_> {
    let mut assets = activity::Assets::new()
        .large_image(view.large_image.as_str())
        .large_text(view.large_text.as_str());
    if let Some(image) = &view.small_image {
        assets = assets.small_image(image.as_str());
    }
    if let Some(text) = &view.small_text {
        assets = assets.small_text(text.as_str());
    }
    let buttons: Vec<activity::Button<'_>> = view
        .buttons
        .iter()
        .map(|button| activity::Button::new(button.label.as_str(), button.url.as_str()))
        .collect();
    activity::Activity::new()
        .details(view.details.as_str())
        .state(view.state.as_str())
        .assets(assets)
        .buttons(buttons)
        .timestamps(activity::Timestamps::new().start(view.start_timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::presence::PresenceButton;

    #[test]
    fn the_activity_carries_the_buttons() {
        let view = PresenceView {
            details: "api · main".to_owned(),
            state: "main.rs".to_owned(),
            large_image: "krad".to_owned(),
            large_text: "Krad ADE".to_owned(),
            small_image: None,
            small_text: None,
            buttons: vec![
                PresenceButton {
                    label: "Get Krad ADE".to_owned(),
                    url: "https://github.com/joordih/krad-ade".to_owned(),
                },
                PresenceButton {
                    label: "View repository".to_owned(),
                    url: "https://github.com/owner/repo".to_owned(),
                },
            ],
            start_timestamp: 1,
        };
        let json = serde_json::to_value(to_activity(&view)).expect("activity json");
        assert_eq!(json["buttons"][0]["label"], "Get Krad ADE");
        assert_eq!(json["buttons"][1]["url"], "https://github.com/owner/repo");
        assert_eq!(json["buttons"].as_array().map(Vec::len), Some(2));
    }
}
