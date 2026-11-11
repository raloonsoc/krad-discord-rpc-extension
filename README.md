# Krad ADE · Discord RPC

A [Krad ADE](https://github.com/joordih/krad-ade) extension that shows your workspace as Discord Rich Presence: the project and its Git branch, the active file with a language icon, how many coding agents are working or waiting, the elapsed session time, and up to two buttons. It updates on file, git and agent events, turns idle after a few quiet minutes, and can be paused from the command palette.

Discord shows it like this:

```text
Krad ADE
krad-discord-rpc-extension · main
main.rs · 2 agents working
1:24:10 elapsed                         [Rust icon]

[ Get Krad ADE ]
```

Discord never shows your own Rich Presence buttons to you, only to the people who can see your activity.

## Features

- **Project and branch:** the group that holds the active file, plus its current branch, e.g. `krad-discord-rpc · main`.
- **File and language:** the active file name and an icon for its language, with an optional cursor line.
- **Agents:** `2 agents working` or `1 agent waiting for input`. Waiting wins over working; a lone agent's harness appears in the icon tooltip.
- **Agent chat icon:** while the editor is not focused and an agent is active, the small icon becomes that agent's icon.
- **Idle:** the presence turns `Idle` after the configured quiet minutes, but never while an agent is working or waiting.
- **Buttons:** `Get Krad ADE` by default, and an optional `View repository` for the project's `origin`.
- **Privacy controls:** hide the project, hide the file, or ignore whole groups.

## Requirements

| Component | Requirement |
| --- | --- |
| Discord | The desktop app, running on the same machine as Krad ADE. The extension talks to its local IPC socket, so the web client does not work. |
| Krad ADE | 0.4.0 or later (extension API `1.2`, app events for agents). |
| OS | macOS on Apple Silicon (`darwin_arm64`). This is the only target `extension.toml` declares today. |
| Rust | A stable Rust toolchain with Cargo, to build from source. |

If Krad ADE does not announce app events, the extension keeps working without agent information and logs a warning.

## Installation

### From GitHub

Not available yet. No releases are published, and `bin/` is git-ignored, so a fresh clone has no binary to run. Once releases exist, you will add the repository URL in Krad ADE, and it will download the binary for your platform from the latest release and check it against the SHA-256 in `extension.toml`.

Until then, build from a local folder.

### From a local folder (development)

1. Clone the repository and build the binary into `bin/`:

   ```sh
   git clone https://github.com/raloonsoc/krad-discord-rpc-extension
   cd krad-discord-rpc-extension
   scripts/install.sh
   ```

   `install.sh` builds in release mode, copies the binary to `bin/krad-discord-rpc-darwin-arm64`, and verifies its signature. It refuses to run on anything but macOS arm64. The file is replaced through a temporary copy so the installed binary gets a new inode: overwriting a binary that Krad ADE is running makes macOS kill the new one with *Code Signature Invalid*.

2. In Krad ADE, open the extension manager, add the project folder, then trust and enable **Discord RPC**.

3. After rebuilding, run `scripts/install.sh` again and reload the extension in Krad ADE.

Build output goes to `../krad-discord-rpc-extension-target`, not to `target/` inside the folder: Krad ADE refuses extension folders over 256 MB and fingerprints every file to decide trust.

## Usage

What the presence shows depends on where you are:

- **In the editor:** the project and branch on the details line, and `file[:line]` on the state line, with agent counts appended when agents are active. The small icon is the file's language.
- **In an agent chat or terminal:** Krad ADE reports no active file when focus leaves the editor. The extension keeps the last file name, and while an agent is active the small icon switches to the agent that needs you most: waiting first, then the most recently updated. A lone agent's tooltip reads e.g. `Claude Code working`.
- **No file open:** the last saved file appears as `Saved lib.rs`, or the agent counts fill the line on their own.
- **Idle:** after `idle-minutes` without activity, and only while no agent is working or waiting, the state line becomes `Idle` and the icon changes. Any activity restores the presence at once.
- **Paused:** the activity is cleared from Discord until you resume.

The session timer counts from when the extension started.

### Commands

Both live in the command palette:

| Command | ID | Effect |
| --- | --- | --- |
| **Discord RPC: Pause / Resume** | `toggle` | Hides the presence immediately; restores it on resume. |
| **Discord RPC: Reconnect** | `reconnect` | Drops the Discord IPC connection and retries right away, useful after restarting Discord. |

Reconnection also happens on its own: the extension retries with exponential backoff from 2 seconds up to 60 seconds while Discord is closed, and clears the activity when paused or when the extension exits.

## Settings

| Key | Default | Effect |
| --- | --- | --- |
| `client-id` | empty | Discord Application ID of your own app. Empty uses the official Krad ADE app (`1569775639412613150`). |
| `show-project` | on | Show the group that holds the active file, or the environment name when no group does. Off hides group, environment and branch names, and the repository button. |
| `show-file` | on | Show the active file, or the last saved one when nothing is open. Off hides file names and the language icon. |
| `show-line` | off | Append `:line` to the file when Krad ADE reports the cursor, which it only does for the file open when the extension started. Requires `show-file`. |
| `show-branch` | on | Append the project's Git branch, e.g. `krad-discord-rpc · main`. Requires `show-project`. |
| `show-agents` | on | Show how many agents are working or waiting, and a lone agent's harness in the tooltip. Off hides the counts and agent icons; agents still keep the presence awake. |
| `idle-minutes` | `5` | Minutes without activity before the presence turns idle (1–120, clamped). |
| `show-krad-button` | on | Add the **Get Krad ADE** button. |
| `show-repo-button` | off | Add a **View repository** button for the active project's `origin` remote. Off by default, since the repository may be private. Requires `show-project`. |
| `ignored-groups` | empty | Comma-separated group names to hide completely. Case-insensitive; agents in those groups do not count either. |
| `enabled` | on | Master switch. Turning it off clears the presence until you turn it back on. |

The branch and the `origin` remote are read from `<project>/.git` without running `git`, including worktrees and submodules where `.git` is a `gitdir:` file. Detached HEAD shows the short commit id. Only `https` remotes are linked; `git@host:owner/repo` and `ssh://git@host/owner/repo` are converted, and embedded credentials are removed.

## Privacy

The extension talks only to the Discord desktop app on the same machine, through its local IPC socket. It runs no server, sends no telemetry, and makes no network requests of its own.

What the presence can contain:

- The project (group) or environment name, the Git branch, the file name (never the full path), and the cursor line if you enable it.
- Agent counts and, for a single agent, its harness name. Terminal ids and the group ids carried by agent events are never shown; the project name already comes from the workspace.
- The elapsed session time.
- The `origin` URL, only when **View repository** is on.
- A fixed link to the Krad ADE repository, from the **Get Krad ADE** button.

What it never reads: file contents, terminal output, or agent prompts. It reads the active project's `.git` files (`HEAD`, `config`, and `commondir` for worktrees) locally, and nothing else from disk.

`show-project` off hides group, environment, branch and repository button. `show-file` off hides file names and the language icon. `ignored-groups` hides the presence entirely while a matching group is active, including its agents. Pause and `enabled` off clear everything.

The small icons are HTTPS URLs to PNGs in [`assets/discord`](assets/discord); Discord's client fetches them from `raw.githubusercontent.com`. That request reaches GitHub, not a server of this extension. If the repository stops being public or the files move off `main`, the icons disappear and the rest of the presence keeps working.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| Nothing appears | Discord desktop is not running, or it runs on another machine | Start Discord desktop on this machine, then run **Discord RPC: Reconnect**. |
| Nothing appears after setting a Client ID | The Application ID is wrong or the app was deleted | Clear `client-id` to use the official app, or paste the ID again. |
| No agent information | Krad ADE is older than 0.4.0 or does not offer app events | Update Krad ADE; the log shows `could not subscribe to app events`. |
| Icons are missing or broken | Discord's asset cache, or the GitHub URLs are unavailable | Run **Reconnect**; missing icons do not break the presence. The repository must stay public with the assets on `main`. |
| No language icon for a file | The extension has no icon for that extension | It falls back to the generic `file` icon. |
| The file name stays after switching to a terminal | Krad ADE reports no active file when focus leaves the editor, and the last one is kept to avoid flicker | Expected. The idle timeout retires it, and active agents take over the icon. |
| Presence stays `Idle` while an agent runs | The agent belongs to a group in `ignored-groups`, or agent events are unavailable | Check `ignored-groups` and the Krad ADE version. |
| Buttons do not show | Discord hides your own Rich Presence buttons from you | Check from another account, or ask someone who can see your activity. |

## Discord application

The extension ships with the official **Krad ADE** Discord application (`1569775639412613150`), so it works without any setup.

To show the activity under your own app instead:

1. Create an application in the [Discord Developer Portal](https://discord.com/developers/applications).
2. Copy **General Information → Application ID** into the **Discord Client ID** setting.
3. Under **Rich Presence → Art Assets**, upload a 512×512 PNG with the key `krad` for the large image.

Leave `client-id` empty to go back to the official app. Only `krad` is a Discord asset key; all small icons are sent as URLs, so they need no upload.

## Icons

Every small icon is a PNG in [`assets/discord`](assets/discord), sent to Discord as an external URL. The asset key is the file name without `.png`.

### Languages

| Asset key | Extensions |
| --- | --- |
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
| `file` | Anything else |
| `idle` | While the workspace is idle |

### Agents

Harness names match ignoring case, spaces, `-` and `_`. For ACP agents the id after `Acp:` is matched by substring. An active agent's tooltip appends `working` or `waiting for input` to the name.

| Harness | Asset key | Name |
| --- | --- | --- |
| `Claude Code`, `ClaudeCode`, `claude-code` | `claude` | Claude Code |
| `opencode` | `opencode` | opencode |
| `Codex` | `agent` | Codex |
| `Shell` | `shell` | Shell |
| `ACP` with `gemini` in the id | `googlegemini` | Gemini |
| `ACP` with `copilot` in the id | `githubcopilot` | Copilot |
| `ACP` with `cursor` in the id | `cursor` | Cursor |
| `ACP` with `mistral` in the id | `mistralai` | Mistral |
| `ACP` with `ollama` in the id | `ollama` | Ollama |
| `ACP` with `claude` in the id | `claude` | Claude |
| `ACP` with any other id | `agent` | The id itself |
| `Gateway`, unknown harness, or none | `agent` | Agent |

The brand icons come from [Simple Icons](https://simpleicons.org), released under CC0; the brands themselves belong to their owners. The `agent`, language, `file` and `idle` icons are drawn for this extension.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
scripts/install.sh
```

Reload the extension in Krad ADE after installing.

### How it is organised

Dependencies point inward: `domain` is pure, `adapters` own the outside world, and `app` wires them together. `main` only starts the app.

```text
src/
  main.rs                     start the Discord thread and the extension
  domain/                     pure model; the caller injects `now`
    event.rs                  the events the extension reacts to
    workspace/                workspace state and how events change it
    agents.rs                 agent snapshots, counts and priority
    settings.rs               defaults and ranges, without the host types
    presence/                 state + settings -> what Discord receives
      text.rs                 file and agent labels, UTF-8 truncation
      icons.rs                language and harness asset catalogue
      buttons.rs              buttons and Discord's limits
  adapters/                   the outside world
    krad/                     host SDK: hooks, settings, conversions
      convert.rs              SDK types -> domain types
      settings.rs             SettingsValues -> Settings
    discord/                  thread that owns the Discord IPC client
      limiter.rs              debounce, rate window and reconnect backoff
      activity.rs             PresenceView -> Discord activity
    git/                      branch and origin read from .git
  app/                        orchestration
    monitor.rs                timer thread: Git refresh and idle
```

Each module keeps its tests in a sibling `tests.rs` when they outgrow the file.
`krad-extension-sdk` is pinned to the Git commit `9c0812c` (release 0.4.0) because the 0.4.0 crate on crates.io predates the app event API. Switch back to the crates.io version in `Cargo.toml` once a release includes it.

There is no release workflow yet. `extension.toml` points at `bin/krad-discord-rpc-darwin-arm64`, which is git-ignored, so publishing needs per-platform binaries plus their `url` and `sha256` entries.

## License

[MIT](LICENSE). Brand icons in `assets/discord/` come from
[Simple Icons](https://simpleicons.org) (CC0); the brands belong to their owners.
