//! Timer thread: refreshes Git data and republishes so the idle timeout can
//! flip the presence with no events at all.

use crate::adapters::discord::DiscordHandle;
use crate::adapters::git;
use crate::app::{lock, publish, Shared};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// A branch (or the idle timeout) can change with no workspace event, so the
/// monitor re-checks them on this interval.
const MONITOR_INTERVAL: Duration = Duration::from_secs(10);

pub(crate) fn spawn(
    shared: Arc<Mutex<Shared>>,
    discord: DiscordHandle,
    wake: Receiver<()>,
) -> std::io::Result<()> {
    thread::Builder::new()
        .name("presence-monitor".to_owned())
        .spawn(move || run(&shared, &discord, &wake))
        .map(|_| ())
}

/// Refreshes Git data, republishes, then sleeps until the next tick or an
/// early wake when the active project changes.
fn run(shared: &Mutex<Shared>, discord: &DiscordHandle, wake: &Receiver<()>) {
    loop {
        refresh_git(shared);
        publish(shared, discord);
        match wake.recv_timeout(MONITOR_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Re-reads the Git facts of the active project. The filesystem work happens
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
