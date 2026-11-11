//! The thread that owns the Discord IPC client: lazy connect, coalesced
//! updates, rate limiting, backoff, and a clean clear on pause or exit.

mod activity;
mod limiter;

use crate::domain::presence::PresenceView;
use crate::domain::settings::Settings;
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use limiter::{Backoff, RateLimiter};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How often the thread wakes to retry the throttle or a connection.
const TICK: Duration = Duration::from_millis(250);

/// Told whether the IPC client is connected, every time that changes.
pub(crate) type ConnectionListener = Arc<dyn Fn(bool) + Send + Sync>;

enum Command {
    Configure {
        client_id: String,
    },
    /// `None` means clear the activity (paused, disabled, ignored group).
    Update(Option<PresenceView>),
    Reconnect,
    /// Registers where connection changes go. Only one listener is kept.
    Watch(ConnectionListener),
    Shutdown,
}

/// A cheap, cloneable sender for handlers.
#[derive(Clone, Debug)]
pub(crate) struct DiscordHandle {
    tx: Sender<Command>,
}

impl DiscordHandle {
    /// Applies a new client id. An empty id disconnects until one is set.
    pub(crate) fn configure(&self, settings: &Settings) {
        self.send(Command::Configure {
            client_id: settings.client_id.clone(),
        });
    }

    /// Queues the newest presence. `None` clears it.
    pub(crate) fn update(&self, view: Option<PresenceView>) {
        self.send(Command::Update(view));
    }

    /// Drops the connection and tries again right away.
    pub(crate) fn reconnect(&self) {
        self.send(Command::Reconnect);
    }

    /// Reports every change of the IPC connection to `listener`, so the
    /// titlebar can say whether Discord is reachable.
    pub(crate) fn watch_connection(&self, listener: ConnectionListener) {
        self.send(Command::Watch(listener));
    }

    fn send(&self, command: Command) {
        // A closed channel only means the process is going away.
        let _ = self.tx.send(command);
    }
}

/// Owner of the Discord thread. Dropping it clears the activity and waits for
/// the thread, so an extension exit does not leave a stale presence behind.
pub(crate) struct Discord {
    handle: DiscordHandle,
    join: Option<JoinHandle<()>>,
}

impl Discord {
    #[must_use]
    pub(crate) fn start() -> Self {
        let (tx, rx) = channel();
        let join = thread::Builder::new()
            .name("discord-rpc".to_owned())
            .spawn(move || run(&rx))
            .ok();
        Self {
            handle: DiscordHandle { tx },
            join,
        }
    }

    #[must_use]
    pub(crate) fn handle(&self) -> DiscordHandle {
        self.handle.clone()
    }

    fn stop(&mut self) {
        let _ = self.handle.tx.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Discord {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(rx: &Receiver<Command>) {
    let mut worker = Worker::default();
    loop {
        match rx.recv_timeout(TICK) {
            Ok(Command::Shutdown) => break,
            Ok(command) => worker.apply(command),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        worker.tick();
    }
    worker.shutdown();
}

#[derive(Default)]
struct Worker {
    client_id: Option<String>,
    client: Option<DiscordIpcClient>,
    /// Newest view the handlers computed; only this one is ever sent.
    desired: Option<PresenceView>,
    /// The view Discord already shows, so identical updates cost nothing.
    applied: Option<PresenceView>,
    limiter: RateLimiter,
    backoff: Backoff,
    listener: Option<ConnectionListener>,
    /// Last connection state told to the listener. `client` starts absent, so
    /// an extension that never reaches Discord reports nothing new.
    announced: bool,
}

impl Worker {
    fn apply(&mut self, command: Command) {
        match command {
            Command::Configure { client_id } => self.configure(client_id),
            Command::Update(view) => {
                if view != self.desired {
                    self.limiter.note_change(Instant::now());
                }
                self.desired = view;
            }
            Command::Reconnect => {
                self.disconnect();
                self.applied = None;
                self.backoff.reset();
            }
            Command::Watch(listener) => self.listener = Some(listener),
            Command::Shutdown => {}
        }
    }

    fn configure(&mut self, client_id: String) {
        let next = Some(client_id).filter(|id| !id.trim().is_empty());
        if next == self.client_id {
            return;
        }
        self.client_id = next;
        self.disconnect();
        self.applied = None;
        self.backoff.reset();
    }

    fn tick(&mut self) {
        self.pump();
        self.announce(self.client.is_some());
    }

    /// Reconciles the connection and the activity with what the handlers want.
    fn pump(&mut self) {
        let Some(client_id) = self.client_id.clone() else {
            return;
        };
        if self.client.is_none() {
            // Nothing to show: do not bother Discord at all.
            if self.desired.is_none() || !self.backoff.ready(Instant::now()) {
                return;
            }
            self.connect(&client_id);
            if self.client.is_none() {
                return;
            }
        }

        let desired = self.desired.clone();
        let throttle_ready = self.limiter.ready(Instant::now());
        let mut lost = false;
        {
            let Some(client) = self.client.as_mut() else {
                return;
            };
            match &desired {
                None => {
                    if self.applied.is_some() {
                        // Clearing on pause is immediate: it is not a
                        // set_activity, so the rate window does not apply.
                        if client.clear_activity().is_err() {
                            lost = true;
                        } else {
                            self.applied = None;
                        }
                    }
                }
                Some(view) if self.applied.as_ref() != Some(view) => {
                    if !throttle_ready {
                        return;
                    }
                    self.limiter.record(Instant::now());
                    if client.set_activity(activity::to_activity(view)).is_err() {
                        lost = true;
                    } else {
                        self.applied = Some(view.clone());
                    }
                }
                Some(_) => {}
            }
        }
        if lost {
            self.connection_lost();
        }
    }

    fn connect(&mut self, client_id: &str) {
        let mut client = DiscordIpcClient::new(client_id);
        match client.connect() {
            Ok(()) => {
                self.client = Some(client);
                self.backoff.reset();
                self.applied = None;
            }
            Err(_) => self.backoff.schedule(Instant::now()),
        }
    }

    fn connection_lost(&mut self) {
        self.disconnect();
        self.applied = None;
        self.backoff.schedule(Instant::now());
    }

    fn disconnect(&mut self) {
        if let Some(mut client) = self.client.take() {
            let _ = client.close();
        }
    }

    fn shutdown(&mut self) {
        if let Some(mut client) = self.client.take() {
            let _ = client.clear_activity();
            let _ = client.close();
        }
        self.applied = None;
    }

    /// Calls the listener only when the connection state differs from the last
    /// one announced.
    fn announce(&mut self, connected: bool) {
        if connected == self.announced {
            return;
        }
        self.announced = connected;
        if let Some(listener) = &self.listener {
            listener(connected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn connection_changes_are_announced_once_each() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let mut worker = Worker {
            listener: Some(Arc::new(move |connected| {
                recorder.lock().expect("seen").push(connected);
            })),
            ..Worker::default()
        };
        // Still the initial state: a start without Discord says nothing.
        worker.announce(false);
        assert!(seen.lock().expect("seen").is_empty());
        worker.announce(true);
        worker.announce(true);
        worker.announce(false);
        assert_eq!(*seen.lock().expect("seen"), [true, false]);
    }

    #[test]
    fn a_worker_without_a_listener_keeps_announcing_quietly() {
        let mut worker = Worker::default();
        worker.announce(true);
        assert!(worker.announced);
    }
}
