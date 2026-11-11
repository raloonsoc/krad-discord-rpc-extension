//! The host adapter: builds the extension and registers every SDK hook.

mod convert;
mod settings;

use crate::adapters::discord::DiscordHandle;
use crate::app::Runtime;
use crate::domain::settings::Settings;
use convert::{app_event, workspace_context, workspace_event};
use krad_extension_sdk::api::protocol::{AppEventKind, LogLevel};
use krad_extension_sdk::api::view::Tone;
use krad_extension_sdk::api::{CommandId, IdError};
use krad_extension_sdk::{App, Extension, HandlerError};
use settings::settings_from_values;
use std::thread;
use std::time::Duration;

const EXTENSION_NAME: &str = "Discord RPC";
const TOGGLE: &str = "toggle";
const RECONNECT: &str = "reconnect";

fn log(app: &App, level: LogLevel, message: impl std::fmt::Display) {
    let _ = app.log(level, format!("{EXTENSION_NAME}: {message}"));
}

fn toast(app: &App, message: &str, tone: Tone) {
    let _ = app.toast(format!("{EXTENSION_NAME}: {message}"), tone);
}

fn command(id: &str) -> Result<CommandId, HandlerError> {
    id.parse::<CommandId>()
        .map_err(|error: IdError| HandlerError::new(error.to_string()))
}

/// Builds the extension: starts the monitor, then registers every host hook.
pub(crate) fn build(handle: DiscordHandle) -> Result<Extension, HandlerError> {
    let (runtime, wake_rx) = Runtime::new(handle);
    if let Err(error) = runtime.start_monitor(wake_rx) {
        eprintln!("krad-discord-rpc: could not start the presence monitor: {error}");
    }

    let init_runtime = runtime.clone();
    let event_runtime = runtime.clone();
    let app_event_runtime = runtime.clone();
    let settings_runtime = runtime.clone();
    let toggle_runtime = runtime.clone();
    let reconnect_runtime = runtime;

    Ok(Extension::new()
        .with_request_timeout(Duration::from_secs(30))
        .on_initialize(move |app, _params| start_loader(app, init_runtime.clone()))
        .on_workspace_event(move |app, event| {
            event_runtime.apply(&workspace_event(&event));
            log(app, LogLevel::Debug, format!("workspace event: {event:?}"));
        })
        .on_app_event(move |app, params| {
            let kind = params.event.kind();
            if let Some(event) = app_event(&params) {
                app_event_runtime.apply(&event);
            }
            log(app, LogLevel::Debug, format!("app event: {kind:?}"));
        })
        .on_settings_changed(move |_app, values| {
            settings_runtime.change_settings(settings_from_values(&values));
        })
        .on_command(command(TOGGLE)?, move |app, _params| {
            let paused = toggle_runtime.toggle_paused();
            toast(app, if paused { "paused" } else { "resumed" }, Tone::Info);
            Ok(())
        })
        .on_command(command(RECONNECT)?, move |app, _params| {
            reconnect_runtime.reconnect();
            toast(app, "reconnecting to Discord", Tone::Info);
            Ok(())
        }))
}

/// Reads settings, the workspace and the app event subscription off the
/// initialize response, so a slow host request never blocks startup.
fn start_loader(app: &App, runtime: Runtime) {
    let worker_app = app.clone();
    let result = thread::Builder::new()
        .name("discord-rpc-init".to_owned())
        .spawn(move || {
            match worker_app.settings() {
                Ok(values) => runtime.set_settings(settings_from_values(&values)),
                Err(error) => {
                    log(
                        &worker_app,
                        LogLevel::Warn,
                        format!("could not read settings, using defaults: {error}"),
                    );
                    runtime.set_settings(Settings::default());
                }
            }
            // workspace/context is a request and can wait; keeping it here,
            // off the initialize response, never blocks startup.
            match worker_app.workspace_context() {
                Ok(context) => runtime.load(&workspace_context(&context)),
                Err(error) => {
                    log(
                        &worker_app,
                        LogLevel::Warn,
                        format!("could not read the workspace context: {error}"),
                    );
                    runtime.publish();
                }
            }
            // App events carry agent activity and Git changes. Subscribe from
            // this worker, never inside the initialize handler, and keep going
            // without agents if the app refuses.
            let subscription = worker_app.subscribe_events([
                AppEventKind::AgentStarted,
                AppEventKind::AgentWaiting,
                AppEventKind::AgentFinished,
                AppEventKind::GitChanged,
                AppEventKind::SessionOpened,
                AppEventKind::SessionClosed,
            ]);
            match subscription {
                Ok(()) => log(&worker_app, LogLevel::Debug, "subscribed to app events"),
                Err(error) => log(
                    &worker_app,
                    LogLevel::Warn,
                    format!("could not subscribe to app events, agents stay hidden: {error}"),
                ),
            }
            // Read the branch of the project that just loaded.
            runtime.refresh_git();
        });
    if let Err(error) = result {
        log(
            app,
            LogLevel::Warn,
            format!("could not start the loader thread: {error}"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::manifest::ExtensionManifest;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn the_manifest_is_valid_and_matches_the_handlers() -> TestResult {
        let manifest: ExtensionManifest = toml::from_str(include_str!("../../../extension.toml"))?;
        manifest.check()?;

        let commands: Vec<&str> = manifest
            .contributes
            .commands
            .iter()
            .map(|command| command.id.as_ref())
            .collect();
        assert_eq!(commands, [TOGGLE, RECONNECT]);

        let settings: Vec<&str> = manifest
            .contributes
            .settings
            .iter()
            .map(|setting| setting.key.as_ref())
            .collect();
        assert_eq!(
            settings,
            [
                "client-id",
                "show-project",
                "show-file",
                "show-line",
                "show-branch",
                "show-agents",
                "idle-minutes",
                "show-krad-button",
                "show-repo-button",
                "ignored-groups",
                "enabled",
            ]
        );
        Ok(())
    }
}
