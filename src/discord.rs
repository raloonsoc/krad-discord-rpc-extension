//! The thread that owns the Discord IPC client.
//!
//! Handlers never touch Discord: they compute a [`PresenceView`] and send it
//! here. This thread connects lazily, coalesces updates (only the newest
//! pending view survives), waits [`DEBOUNCE`] so a burst costs one call, keeps
//! under Discord's limit of [`RATE_LIMIT`] calls per [`RATE_WINDOW`], and reconnects with exponential backoff while Discord is not
//! running. It never panics and never writes to stdout, which belongs to the
//! JSON-RPC channel.

use crate::presence::PresenceView;
use crate::settings::Settings;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Discord accepts 5 `set_activity` calls per 20 seconds.
pub const RATE_LIMIT: usize = 5;
pub const RATE_WINDOW: Duration = Duration::from_secs(20);
/// A new view waits this long so a burst of events costs a single call.
pub const DEBOUNCE: Duration = Duration::from_secs(1);
const INITIAL_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// How often the thread wakes to retry the throttle or a connection.
const TICK: Duration = Duration::from_millis(250);

enum Command {
    Configure {
        client_id: String,
    },
    /// `None` means clear the activity (paused, disabled, ignored group).
    Update(Option<PresenceView>),
    Reconnect,
    Shutdown,
}

/// A cheap, cloneable sender for handlers.
#[derive(Clone, Debug)]
pub struct DiscordHandle {
    tx: Sender<Command>,
}

impl DiscordHandle {
    /// Applies a new client id. An empty id disconnects until one is set.
    pub fn configure(&self, settings: &Settings) {
        self.send(Command::Configure {
            client_id: settings.client_id.clone(),
        });
    }

    /// Queues the newest presence. `None` clears it.
    pub fn update(&self, view: Option<PresenceView>) {
        self.send(Command::Update(view));
    }

    /// Drops the connection and tries again right away.
    pub fn reconnect(&self) {
        self.send(Command::Reconnect);
    }

    fn send(&self, command: Command) {
        // A closed channel only means the process is going away.
        let _ = self.tx.send(command);
    }
}

/// Owner of the Discord thread. Dropping it clears the activity and waits for
/// the thread, so an extension exit does not leave a stale presence behind.
pub struct Discord {
    handle: DiscordHandle,
    join: Option<JoinHandle<()>>,
}

impl Discord {
    #[must_use]
    pub fn start() -> Self {
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
    pub fn handle(&self) -> DiscordHandle {
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

struct Worker {
    client_id: Option<String>,
    client: Option<DiscordIpcClient>,
    /// Newest view the handlers computed; only this one is ever sent.
    desired: Option<PresenceView>,
    /// The view Discord already shows, so identical updates cost nothing.
    applied: Option<PresenceView>,
    /// When each recent `set_activity` went out, oldest first.
    sent: VecDeque<Instant>,
    /// When `desired` last changed, for the debounce.
    changed_at: Option<Instant>,
    retry_at: Option<Instant>,
    backoff: Duration,
}

impl Default for Worker {
    fn default() -> Self {
        Self {
            client_id: None,
            client: None,
            desired: None,
            applied: None,
            sent: VecDeque::new(),
            changed_at: None,
            retry_at: None,
            backoff: INITIAL_BACKOFF,
        }
    }
}

impl Worker {
    fn apply(&mut self, command: Command) {
        match command {
            Command::Configure { client_id } => self.configure(client_id),
            Command::Update(view) => {
                if view != self.desired {
                    self.changed_at = Some(Instant::now());
                }
                self.desired = view;
            }
            Command::Reconnect => {
                self.disconnect();
                self.applied = None;
                self.retry_at = None;
                self.backoff = INITIAL_BACKOFF;
            }
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
        self.retry_at = None;
        self.backoff = INITIAL_BACKOFF;
    }

    fn tick(&mut self) {
        let Some(client_id) = self.client_id.clone() else {
            return;
        };
        if self.client.is_none() {
            // Nothing to show: do not bother Discord at all.
            if self.desired.is_none() || self.retry_at.is_some_and(|at| at > Instant::now()) {
                return;
            }
            self.connect(&client_id);
            if self.client.is_none() {
                return;
            }
        }

        let desired = self.desired.clone();
        let throttle_ready = self.ready(Instant::now());
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
                    self.sent.push_back(Instant::now());
                    if client.set_activity(activity_for(view)).is_err() {
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

    /// True when the debounce has passed and the rate window has room.
    fn ready(&mut self, now: Instant) -> bool {
        while self
            .sent
            .front()
            .is_some_and(|sent| now.duration_since(*sent) >= RATE_WINDOW)
        {
            self.sent.pop_front();
        }
        let settled = self
            .changed_at
            .is_none_or(|changed| now.duration_since(changed) >= DEBOUNCE);
        settled && self.sent.len() < RATE_LIMIT
    }

    fn connect(&mut self, client_id: &str) {
        let mut client = DiscordIpcClient::new(client_id);
        match client.connect() {
            Ok(()) => {
                self.client = Some(client);
                self.retry_at = None;
                self.backoff = INITIAL_BACKOFF;
                self.applied = None;
            }
            Err(_) => {
                self.retry_at = Some(Instant::now() + self.backoff);
                self.backoff = grow_backoff(self.backoff);
            }
        }
    }

    fn connection_lost(&mut self) {
        self.disconnect();
        self.applied = None;
        self.retry_at = Some(Instant::now() + self.backoff);
        self.backoff = grow_backoff(self.backoff);
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
}

fn grow_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_BACKOFF)
}

fn activity_for(view: &PresenceView) -> activity::Activity<'_> {
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
    use crate::presence::PresenceButton;

    #[test]
    fn updates_wait_the_debounce_and_respect_the_rate_window() {
        let mut worker = Worker::default();
        let start = Instant::now();
        worker.changed_at = Some(start);
        assert!(!worker.ready(start));
        assert!(worker.ready(start + DEBOUNCE));
        for _ in 0..RATE_LIMIT {
            worker.sent.push_back(start);
        }
        assert!(!worker.ready(start + DEBOUNCE));
        assert!(worker.ready(start + RATE_WINDOW));
    }

    #[test]
    fn backoff_doubles_and_stops_at_a_minute() {
        let mut backoff = INITIAL_BACKOFF;
        let mut seen = vec![backoff];
        for _ in 0..10 {
            backoff = grow_backoff(backoff);
            seen.push(backoff);
        }
        assert_eq!(
            seen[..5],
            [
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                Duration::from_secs(32),
            ]
        );
        assert_eq!(backoff, MAX_BACKOFF);
        assert_eq!(seen.last(), Some(&MAX_BACKOFF));
    }

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
        let json = serde_json::to_value(activity_for(&view)).expect("activity json");
        assert_eq!(json["buttons"][0]["label"], "Get Krad ADE");
        assert_eq!(json["buttons"][1]["url"], "https://github.com/owner/repo");
        assert_eq!(json["buttons"].as_array().map(Vec::len), Some(2));
    }
}
