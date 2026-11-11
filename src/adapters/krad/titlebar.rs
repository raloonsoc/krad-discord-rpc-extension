//! Keeps the titlebar button's highlight and tooltip in sync with the
//! indicator the app computed. Sends only when the indicator changes.

use crate::domain::indicator::Indicator;
use krad_extension_sdk::api::protocol::{LogLevel, SetTitlebarItemParams};
use krad_extension_sdk::api::TitlebarItemId;
use krad_extension_sdk::App;

/// Declared in `extension.toml` under `[[contributes.titlebar_item]]`.
pub(crate) const TITLEBAR_ITEM: &str = "discord-rpc";

/// The host handle plus the last indicator it accepted. The handle only exists
/// once the extension has initialized.
#[derive(Default)]
pub(crate) struct Titlebar {
    app: Option<App>,
    /// Parsed once; the manifest and this constant share the id.
    item: Option<TitlebarItemId>,
    sent: Option<Indicator>,
}

impl Titlebar {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            item: TITLEBAR_ITEM.parse().ok(),
            ..Self::default()
        }
    }

    /// Stores the host handle. The next [`Self::show`] sends the state, since
    /// nothing has been sent yet.
    pub(crate) fn attach(&mut self, app: App) {
        self.app = Some(app);
    }

    /// Sends the indicator when it differs from the one already shown.
    pub(crate) fn show(&mut self, wanted: &Indicator) {
        if self.sent.as_ref() == Some(wanted) {
            return;
        }
        let (Some(app), Some(item)) = (&self.app, &self.item) else {
            return;
        };
        let params = SetTitlebarItemParams {
            item: item.clone(),
            icon: None,
            title: Some(wanted.tooltip.clone()),
            active: Some(wanted.active),
            hidden: None,
        };
        match app.set_titlebar_item(&params) {
            Ok(()) => self.sent = Some(wanted.clone()),
            Err(error) => {
                // Keep `sent` untouched so the next publish retries.
                let _ = app.log(
                    LogLevel::Warn,
                    format!(
                        "{}: could not update the titlebar: {error}",
                        super::EXTENSION_NAME
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_item_id_is_a_valid_titlebar_id() {
        assert_eq!(Titlebar::new().item.expect("item").as_ref(), TITLEBAR_ITEM);
    }

    #[test]
    fn nothing_is_sent_before_the_host_handle_arrives() {
        let mut titlebar = Titlebar::new();
        let wanted = crate::domain::indicator::indicator(false, true, true);
        titlebar.show(&wanted);
        assert!(titlebar.sent.is_none());
    }
}
