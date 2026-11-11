//! Krad ADE extension that shows the workspace as Discord Rich Presence.
//! Starting up is all this file does; nothing goes to stdout (JSON-RPC).

mod adapters;
mod app;
mod domain;

use adapters::discord::Discord;
use std::process::ExitCode;

fn main() -> ExitCode {
    let discord = Discord::start();
    let extension = match adapters::krad::build(discord.handle()) {
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
