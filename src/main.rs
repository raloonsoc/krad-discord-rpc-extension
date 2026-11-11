//! Krad ADE extension that shows the workspace as Discord Rich Presence.
//!
//! Handlers only update [`state::State`] and compute a [`presence::PresenceView`];
//! a dedicated thread in [`discord`] owns the Discord IPC client. Nothing is
//! printed to stdout, which is the JSON-RPC channel: unexpected failures go to
//! stderr, expected ones to the app log.

mod discord;
mod git;
mod presence;
mod settings;
mod state;

use discord::{Discord, DiscordHandle};
use krad_extension_sdk::api::protocol::{AppEventKind, LogLevel, SettingsValues};
use krad_extension_sdk::api::view::Tone;
use krad_extension_sdk::api::{CommandId, IdError};
use krad_extension_sdk::{App, Extension, HandlerError};
use settings::Settings;
use state::State;
use std::process::ExitCode;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime};

const EXTENSION_NAME: &str = "Discord RPC";
const TOGGLE: &str = "toggle";
const RECONNECT: &str = "reconnect";

/// A branch (or the idle timeout) can change with no workspace event, so the
/// monitor re-checks them on this interval.
const MONITOR_INTERVAL: Duration = Duration::from_secs(10);

struct Shared {
    state: State,
    settings: Settings,
}

impl Shared {
    fn new() -> Self {
        Self {
            state: State::new(SystemTime::now()),
            settings: Settings::default(),
        }
    }
}

type SharedState = Arc<Mutex<Shared>>;

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

fn log(app: &App, level: LogLevel, message: impl std::fmt::Display) {
    let _ = app.log(level, format!("{EXTENSION_NAME}: {message}"));
}

fn toast(app: &App, message: &str, tone: Tone) {
    let _ = app.toast(format!("{EXTENSION_NAME}: {message}"), tone);
}

/// Recomputes the presence from the shared state and queues it for Discord.
fn publish(shared: &Mutex<Shared>, discord: &DiscordHandle) {
    let view = {
        let shared = lock(shared);
        presence::view(&shared.state, &shared.settings, SystemTime::now())
    };
    discord.update(view);
}

/// Re-reads the git facts of the active project. The filesystem work happens
/// without the lock, and the result only lands if the project did not change
/// meanwhile (the next tick would fix it anyway).
fn refresh_git(shared: &Mutex<Shared>) {
    let root = lock(shared)
        .state
        .active_group()
        .map(|group| group.root.clone());
    let info = root
        .as_deref()
        .map_or_else(git::GitInfo::default, git::info);
    let mut shared = lock(shared);
    let current = shared.state.active_group().map(|group| group.root.clone());
    if current == root {
        shared.state.branch = info.branch;
        shared.state.repo_url = info.remote_url;
    }
}

/// Timer thread: refreshes git data and republishes so the idle timeout can
/// flip the presence with no events at all. `wake` refreshes early when the
/// active project changes.
fn monitor(shared: &Mutex<Shared>, discord: &DiscordHandle, wake: &Receiver<()>) {
    loop {
        refresh_git(shared);
        publish(shared, discord);
        match wake.recv_timeout(MONITOR_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn apply_settings(shared: &Mutex<Shared>, discord: &DiscordHandle, values: &SettingsValues) {
    let settings = Settings::from_values(values);
    discord.configure(&settings);
    lock(shared).settings = settings;
    publish(shared, discord);
}

fn command(id: &str) -> Result<CommandId, HandlerError> {
    id.parse::<CommandId>()
        .map_err(|error: IdError| HandlerError::new(error.to_string()))
}

fn build(handle: DiscordHandle) -> Result<Extension, HandlerError> {
    let shared: SharedState = Arc::new(Mutex::new(Shared::new()));
    let (wake_tx, wake_rx) = channel();

    let init_shared = Arc::clone(&shared);
    let init_discord = handle.clone();
    let init_wake = wake_tx.clone();
    let event_shared = Arc::clone(&shared);
    let event_discord = handle.clone();
    let event_wake = wake_tx.clone();
    let app_event_shared = Arc::clone(&shared);
    let app_event_discord = handle.clone();
    let app_event_wake = wake_tx;
    let settings_shared = Arc::clone(&shared);
    let settings_discord = handle.clone();
    let toggle_shared = Arc::clone(&shared);
    let toggle_discord = handle.clone();
    let reconnect_shared = Arc::clone(&shared);
    let reconnect_discord = handle.clone();

    let monitor_shared = Arc::clone(&shared);
    let monitor_discord = handle;
    if let Err(error) = thread::Builder::new()
        .name("presence-monitor".to_owned())
        .spawn(move || monitor(&monitor_shared, &monitor_discord, &wake_rx))
    {
        eprintln!("krad-discord-rpc: could not start the presence monitor: {error}");
    }

    Ok(Extension::new()
        .with_request_timeout(Duration::from_secs(30))
        .on_initialize(move |app, _params| {
            let worker_app = app.clone();
            let worker_shared = Arc::clone(&init_shared);
            let worker_discord = init_discord.clone();
            let worker_wake = init_wake.clone();
            let result = thread::Builder::new()
                .name("discord-rpc-init".to_owned())
                .spawn(move || {
                    match worker_app.settings() {
                        Ok(values) => {
                            let settings = Settings::from_values(&values);
                            worker_discord.configure(&settings);
                            lock(&worker_shared).settings = settings;
                        }
                        Err(error) => {
                            log(
                                &worker_app,
                                LogLevel::Warn,
                                format!("could not read settings, using defaults: {error}"),
                            );
                            worker_discord.configure(&Settings::default());
                        }
                    }
                    // workspace/context is a request and can wait; keeping it
                    // here, off the initialize response, never blocks startup.
                    match worker_app.workspace_context() {
                        Ok(context) => {
                            lock(&worker_shared).state.load(&context, SystemTime::now());
                            publish(&worker_shared, &worker_discord);
                        }
                        Err(error) => {
                            log(
                                &worker_app,
                                LogLevel::Warn,
                                format!("could not read the workspace context: {error}"),
                            );
                            publish(&worker_shared, &worker_discord);
                        }
                    }
                    // App events carry agent activity and Git changes. Subscribe
                    // from this worker, never inside the initialize handler, and
                    // keep going without agents if the app refuses.
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
                            format!(
                                "could not subscribe to app events, agents stay hidden: {error}"
                            ),
                        ),
                    }
                    // Read the branch of the project that just loaded.
                    let _ = worker_wake.send(());
                });
            if let Err(error) = result {
                log(
                    app,
                    LogLevel::Warn,
                    format!("could not start the loader thread: {error}"),
                );
            }
        })
        .on_workspace_event(move |app, event| {
            lock(&event_shared).state.apply(&event, SystemTime::now());
            log(app, LogLevel::Debug, format!("workspace event: {event:?}"));
            // The active project may have changed: re-read its git facts now.
            let _ = event_wake.send(());
            publish(&event_shared, &event_discord);
        })
        .on_app_event(move |app, params| {
            let kind = params.event.kind();
            lock(&app_event_shared)
                .state
                .apply_app_event(&params, SystemTime::now());
            log(app, LogLevel::Debug, format!("app event: {kind:?}"));
            // A commit, checkout or branch switch: refresh git right away.
            if kind == AppEventKind::GitChanged {
                let _ = app_event_wake.send(());
            }
            publish(&app_event_shared, &app_event_discord);
        })
        .on_settings_changed(move |_app, values| {
            apply_settings(&settings_shared, &settings_discord, &values);
        })
        .on_command(command(TOGGLE)?, move |app, _params| {
            let paused = {
                let mut shared = lock(&toggle_shared);
                shared.state.mark_activity(SystemTime::now());
                shared.state.toggle_paused()
            };
            publish(&toggle_shared, &toggle_discord);
            toast(app, if paused { "paused" } else { "resumed" }, Tone::Info);
            Ok(())
        })
        .on_command(command(RECONNECT)?, move |app, _params| {
            lock(&reconnect_shared)
                .state
                .mark_activity(SystemTime::now());
            reconnect_discord.reconnect();
            publish(&reconnect_shared, &reconnect_discord);
            toast(app, "reconnecting to Discord", Tone::Info);
            Ok(())
        }))
}

fn main() -> ExitCode {
    let discord = Discord::start();
    let extension = match build(discord.handle()) {
        Ok(extension) => extension,
        Err(error) => {
            eprintln!("krad-discord-rpc: {error}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = extension.run();
    // Clears the activity on Discord and joins the owner thread.
    drop(discord);
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("krad-discord-rpc: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::manifest::ExtensionManifest;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn the_manifest_is_valid_and_matches_the_handlers() -> TestResult {
        let manifest: ExtensionManifest = toml::from_str(include_str!("../extension.toml"))?;
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
