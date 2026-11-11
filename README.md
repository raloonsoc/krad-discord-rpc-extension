# Krad ADE · Discord RPC

Shows your Krad ADE workspace as Discord Rich Presence: the project and its Git
branch, the active file (optionally with the cursor line), a language icon, how
many coding agents are working or waiting for input, the session elapsed time
and up to two buttons. It updates when you switch files, save, change groups or
an agent changes state, turns idle after a few quiet minutes, and it can be
paused from the command palette.

> Discord never shows your own Rich Presence buttons to you, only to the rest
> of the people who can see your activity. The **Get Krad ADE** and **View
> repository** buttons are there for everybody else.

## Requirements

- Discord **desktop** running on the same machine (the extension talks to its
  local IPC socket; the web client does not work).
- macOS arm64 for the prebuilt target.
- Rust and Cargo to build from source.

## Discord application

The extension ships with the official **Krad ADE** Discord application
(`1569775639412613150`), so it works without any setup. To show your activity
under your own app instead, create one in the
[Discord Developer Portal](https://discord.com/developers/applications), copy
**General Information → Application ID** and paste it into the **Discord Client
ID** setting. Leave the setting empty to go back to the official app.

## Rich Presence assets

Upload these names under **Rich Presence → Art Assets** in the Developer Portal.
Only the large image (`krad`) is sent as an asset key. The small icons are sent
as HTTPS URLs to the PNGs in [`assets/discord`](assets/discord) because Discord's
client cache does not reliably resolve freshly uploaded keys; Discord turns them
into external images. Loading them by URL requires this repository to stay
public and the assets to remain on `main`; if they are unavailable, the presence
still works without small icons.

| Asset key | Used for |
| --- | --- |
| `krad` | Large image: Krad ADE |
| `file` | Fallback small image |
| `idle` | Small image while the workspace is idle |
| `rust` | `.rs` |
| `typescript` | `.ts`, `.mts`, `.cts` |
| `react` | `.tsx`, `.jsx` |
| `javascript` | `.js`, `.mjs`, `.cjs` |
| `python` | `.py`, `.pyi` |
| `go` | `.go` |
| `markdown` | `.md`, `.mdx` |
| `toml` | `.toml` |
| `json` | `.json`, `.jsonc` |
| `yaml` | `.yaml`, `.yml` |
| `shell` | `.sh`, `.bash`, `.zsh`, `.fish` |
| `c` | `.c`, `.h` |
| `cpp` | `.cc`, `.cpp`, `.cxx`, `.hpp`, `.hh` |
| `java` | `.java` |
| `kotlin` | `.kt`, `.kts` |
| `swift` | `.swift` |
| `html` | `.html`, `.htm` |
| `css` | `.css` |
| `sass` | `.scss`, `.sass` |
| `php` | `.php` |
| `ruby` | `.rb` |
| `csharp` | `.cs` |
| `sql` | `.sql` |
| `vue` | `.vue` |
| `svelte` | `.svelte` |
| `lua` | `.lua` |
| `zig` | `.zig` |
| `elixir` | `.ex`, `.exs` |

PNG or JPG, at least 512×512 for `krad` and 128×128 for the small icons is
recommended. Missing assets do not break anything: Discord simply shows no icon
for that key.

## Coding agents

The extension subscribes to the app events Krad ADE announces for agents
(`agent_started`, `agent_waiting`, `agent_finished`), and to Git and session
changes. While at least one agent is working or waiting for input the presence
never turns idle, however long the agent runs.

- With an active file: `main.rs · 2 agents working`.
- Without one: `1 agent waiting for input`.
- Waiting agents win over working ones, because they need you.
- When a single agent is active its harness appears in the small icon tooltip,
  e.g. `Claude Code working`; with several agents only the count is shown.

Agents that belong to a group listed under **Ignored groups** do not count, and
their names are never shown. If Krad ADE does not offer the `events` feature the
extension logs a warning and keeps working without agent information.

## Settings

| Key | Default | Effect |
| --- | --- | --- |
| `client-id` | empty | Optional Discord Application ID of your own app. Empty uses the official Krad ADE app (`1569775639412613150`). |
| `show-project` | on | Show the group (project) containing the active file. Off hides all project, environment and branch names, and the repository button. |
| `show-file` | on | Show the active file, or the last saved one when nothing is open. Off hides file names and icons. |
| `show-line` | off | Append `:line` to the file. Requires `show-file`. |
| `show-branch` | on | Append the active project's Git branch to its name, e.g. `krad-discord-rpc · main`. Requires `show-project`. |
| `show-agents` | on | Show how many coding agents are working or waiting, e.g. `main.rs · 2 agents working`. The harness name appears only when a single agent is active. Agents keep the presence awake even when this is off. |
| `idle-minutes` | 5 | Minutes without activity before the presence turns idle (1–120). Any activity brings it back at once. |
| `show-krad-button` | on | Add the **Get Krad ADE** button. |
| `show-repo-button` | off | Add a **View repository** button for the active project's `origin` remote. Off by default: the repository may be private. |
| `ignored-groups` | empty | Comma-separated group names whose presence is hidden completely. Case-insensitive. |
| `enabled` | on | Master switch for the presence. |

The branch and the `origin` remote are read from `<project>/.git` without
running `git`, including worktrees and submodules where `.git` is a `gitdir:`
file. Only `https` remotes are linked; `git@host:owner/repo` and
`ssh://git@host/owner/repo` are converted to `https://host/owner/repo`.

## Commands

- **Discord RPC: Pause / Resume** (`toggle`) hides the presence immediately and
  restores it on resume.
- **Discord RPC: Reconnect** (`reconnect`) drops the IPC connection and retries,
  useful after restarting Discord.

Reconnection otherwise happens on its own: the extension retries with
exponential backoff from 2 s up to 60 s while Discord is closed, and it clears
the activity when paused or when the extension exits.

## Build, test and install

```sh
cargo test
cargo clippy --all-targets -- -D warnings
scripts/install.sh
```

`krad-extension-sdk` is pinned to a Git commit until a release with the app
event API is published on crates.io. `install.sh` builds in release mode, copies
the binary to `bin/krad-discord-rpc-darwin-arm64` and verifies its signature.
Reload the extension in Krad ADE afterwards.

## How it is organised

- `settings.rs` reads the contributed settings.
- `state.rs` is a pure model of the workspace: the startup context, every
  workspace event and the agent snapshots fed by the app events. A finished
  agent stays as idle instead of being removed.
- `git.rs` reads the branch and the `origin` remote straight from `.git`,
  resolving `gitdir:` files and worktree `commondir` files without running
  `git`.
- `presence.rs` turns state + settings + the current time into a `PresenceView`,
  with no IO. It sends the small icons as external URLs and truncates every text
  to Discord's 128-character limit.
- `discord.rs` owns the Discord IPC client on its own thread: lazy connect,
  backoff, coalesced updates (1 s debounce, at most 5 per 20 s as Discord allows), clear on pause/exit.
- `main.rs` wires the SDK hooks and commands, subscribes to app events from the
  loader thread, and runs a 10 s monitor thread that refreshes the branch, the
  remote and the idle state. A `git_changed` event wakes it immediately.
