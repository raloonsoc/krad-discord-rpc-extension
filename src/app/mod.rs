//! Orchestration: shared workspace state, publishing and the palette commands.

use crate::adapters::discord::DiscordHandle;
use crate::adapters::krad::titlebar::Titlebar;
use crate::domain::event::Event;
use crate::domain::indicator::{indicator, Indicator};
use crate::domain::settings::Settings;
use crate::domain::workspace::{State, WorkspaceContext};
use krad_extension_sdk::App;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

pub(crate) mod monitor;

pub(crate) struct Shared {
    pub(crate) state: State,
    pub(crate) settings: Settings,
    /// Whether the Discord IPC client is connected right now, told by the
    /// Discord thread. The titlebar button reads it.
    connected: bool,
    /// The titlebar button: the host handle and the last indicator sent.
    titlebar: Titlebar,
}

impl Shared {
    fn new() -> Self {
        Self {
            state: State::new(SystemTime::now()),
            settings: Settings::default(),
            connected: false,
            titlebar: Titlebar::new(),
        }
    }

    fn indicator(&self) -> Indicator {
        indicator(self.state.paused, self.settings.enabled, self.connected)
    }
}

type SharedState = Arc<Mutex<Shared>>;

pub(crate) fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Recomputes the presence from the shared state and queues it for Discord.
/// The titlebar button follows the same state, so it updates here too.
pub(crate) fn publish(shared: &Mutex<Shared>, discord: &DiscordHandle) {
    let (view, indicator) = {
        let shared = lock(shared);
        (
            crate::domain::presence::view(&shared.state, &shared.settings, SystemTime::now()),
            shared.indicator(),
        )
    };
    discord.update(view);
    // The titlebar adapter drops identical states, so this is cheap.
    lock(shared).titlebar.show(&indicator);
}

/// Stores the IPC connection state and republishes only when it changed.
fn set_connected(shared: &Mutex<Shared>, discord: &DiscordHandle, connected: bool) {
    let changed = {
        let mut shared = lock(shared);
        let changed = shared.connected != connected;
        shared.connected = connected;
        changed
    };
    if changed {
        publish(shared, discord);
    }
}

/// Everything the host handlers need, cheap to clone for each callback.
#[derive(Clone)]
pub(crate) struct Runtime {
    shared: SharedState,
    discord: DiscordHandle,
    wake: Sender<()>,
}

impl Runtime {
    pub(crate) fn new(discord: DiscordHandle) -> (Self, Receiver<()>) {
        let (wake, wake_rx) = channel();
        let shared = Arc::new(Mutex::new(Shared::new()));
        let watch_shared = Arc::clone(&shared);
        let watch_discord = discord.clone();
        discord.watch_connection(Arc::new(move |connected| {
            set_connected(&watch_shared, &watch_discord, connected);
        }));
        (
            Self {
                shared,
                discord,
                wake,
            },
            wake_rx,
        )
    }

    /// Hands the titlebar button the host handle and shows the current state.
    /// The host only exists once the extension initializes.
    pub(crate) fn attach(&self, app: &App) {
        let mut shared = lock(&self.shared);
        shared.titlebar.attach(app.clone());
        let indicator = shared.indicator();
        shared.titlebar.show(&indicator);
    }

    /// Starts the timer thread that keeps Git and idle fresh.
    pub(crate) fn start_monitor(&self, wake: Receiver<()>) -> std::io::Result<()> {
        monitor::spawn(Arc::clone(&self.shared), self.discord.clone(), wake)
    }

    /// Applies one event, refreshing Git when it could have changed, and
    /// republishes the presence.
    pub(crate) fn apply(&self, event: &Event) {
        lock(&self.shared).state.apply(event, SystemTime::now());
        if event.wants_git_refresh() {
            self.refresh_git();
        }
        self.publish();
    }

    /// Replaces the workspace with the startup context and republishes.
    pub(crate) fn load(&self, context: &WorkspaceContext) {
        lock(&self.shared).state.load(context, SystemTime::now());
        self.publish();
    }

    /// Stores settings without publishing, for the startup path.
    pub(crate) fn set_settings(&self, settings: Settings) {
        self.discord.configure(&settings);
        lock(&self.shared).settings = settings;
    }

    /// Stores settings and republishes, for a settings change.
    pub(crate) fn change_settings(&self, settings: Settings) {
        self.set_settings(settings);
        self.publish();
    }

    /// Flips pause and returns the new value.
    pub(crate) fn toggle_paused(&self) -> bool {
        let paused = {
            let mut shared = lock(&self.shared);
            shared.state.mark_activity(SystemTime::now());
            shared.state.toggle_paused()
        };
        self.publish();
        paused
    }

    /// Drops the Discord connection and tries again right away.
    pub(crate) fn reconnect(&self) {
        lock(&self.shared).state.mark_activity(SystemTime::now());
        self.discord.reconnect();
        self.publish();
    }

    pub(crate) fn publish(&self) {
        publish(&self.shared, &self.discord);
    }

    /// Wakes the monitor so it re-reads the active project's Git facts.
    pub(crate) fn refresh_git(&self) {
        let _ = self.wake.send(());
    }
}
