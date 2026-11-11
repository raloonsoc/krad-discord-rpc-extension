# Krad ADE · Discord RPC

A [Krad ADE](https://github.com/joordih/krad-ade) extension that shows your workspace as Discord Rich Presence: the project and its Git branch, the active file with a language icon, how many coding agents are working or waiting, the elapsed session time, and up to two buttons. It updates on file, git and agent events, turns idle after a few quiet minutes, and can be paused from the command palette or the titlebar button.

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
- **Titlebar button:** a Discord RPC button that pauses and resumes, highlighted while the presence is shared.
- **Privacy controls:** hide the project, hide the file, or ignore whole groups.

## Requirements

| Component | Requirement |
| --- | --- |
| Discord | The desktop app, running on the same machine as Krad ADE. The extension talks to its local IPC socket, so the web client does not work. |
| Krad ADE | 0.4.0 or later (extension API `1.2`, app events for agents). |
| OS | macOS, Linux (x64 or arm64) or Windows (x64 or arm64). |
| Rust | A stable Rust toolchain with Cargo, to build from source. |

If Krad ADE does not announce app events, the extension keeps working without agent information and logs a warning.

## Installation

### From GitHub (recommended)

1. In Krad ADE, open the extension manager and add the repository URL:

   ```
   https://github.com/raloonsoc/krad-discord-rpc-extension
   ```

2. Trust and enable **Discord RPC**.

Krad ADE clones the default branch (`main`) into `~/.config/krad-ade/extensions/discord-rpc`. On first start it downloads the binary for your platform from the latest GitHub Release, checks it against the SHA-256 in `extension.toml`, and keeps it in its program cache. Nothing is built on your machine. **Update** in the extension manager pulls `main` and asks for trust again.

Binaries exist for `darwin_arm64`, `darwin_x64`, `linux_x64`, `linux_arm64`, `win_x64` and `win_arm64`; they are published with the first version tag. Until then `extension.toml` points at the local `bin/` paths and installing from GitHub cannot work, so build from a local folder instead.

### From a local folder (development)

1. Clone the repository and build the binary into `bin/`:

   ```sh
   git clone https://github.com/raloonsoc/krad-discord-rpc-extension
   cd krad-discord-rpc-extension
   scripts/install.sh          # macOS or Linux, this machine's architecture
   scripts/install.sh all      # ...or both arm64 and x64
   ```

   On Windows, use PowerShell instead:

   ```powershell
   scripts\install.ps1              # this machine's architecture
   scripts\install.ps1 -Arch x64    # ...or a specific one
   ```

   The script builds in release mode, copies the binary to `bin/krad-discord-rpc-<os>-<arch>`, and on macOS verifies its signature. Building the other architecture needs its Rust target first, e.g. `rustup target add x86_64-apple-darwin`. `binaries.py local` then points `extension.toml` at the files in `bin/`; do not commit that change after a release. The file is replaced through a temporary copy so the installed binary gets a new inode: overwriting one that Krad ADE is running makes macOS kill the new process with *Code Signature Invalid*.

2. In Krad ADE, open the extension manager, add the project folder, then trust and enable **Discord RPC**.

3. After rebuilding, run the install script again and reload the extension in Krad ADE.

Build output goes to `../krad-discord-rpc-extension-target`, not to `target/` inside the folder: Krad ADE refuses extension folders over 256 MB and fingerprints every file to decide trust.

## Usage

What the presence shows depends on where you are:

- **In the editor:** the project and branch on the details line, and `file[:line]` on the state line, with agent counts appended when agents are active. The small icon is the file's language.
- **In an agent chat or terminal:** Krad ADE reports no active file when focus leaves the editor. The extension keeps the last file name, and while an agent is active the small icon switches to the agent that needs you most: waiting first, then the most recently updated. A lone agent's tooltip reads e.g. `Claude Code working`.
- **No file open:** the last saved file appears as `Saved lib.rs`, or the agent counts fill the line on their own.
- **Idle:** after `idle-minutes` without activity, and only while no agent is working or waiting, the state line becomes `Idle` and the icon changes. Any activity restores the presence at once.
- **Paused:** the activity is cleared from Discord until you resume.

The button next to the ADE/Editor switch mirrors the same switches: it stays highlighted and reads `Discord RPC: sharing` while the presence is shown, turns dim with `Discord RPC: paused` when paused or with `enabled` off, and reads `Discord RPC: Discord not running` while the extension cannot reach the Discord IPC socket. Clicking it runs the same pause/resume command.

The session timer counts from when the extension started.

### Commands

Both live in the command palette:

| Command | ID | Effect |
| --- | --- | --- |
| **Discord RPC: Pause / Resume** | `toggle` | Hides the presence immediately; restores it on resume. Bound to the titlebar button. |
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
cargo fmt --check
scripts/install.sh
```

Reload the extension in Krad ADE after installing. CI (`.github/workflows/ci.yml`) runs the format check, clippy and the tests on macOS, Ubuntu and Windows for every push and pull request to `main`.

`krad-extension-sdk` comes from the Git commit pinned in `Cargo.toml`, so a build needs a Git client; the CI runners have one.

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
    indicator.rs              the titlebar button's highlight and tooltip
    presence/                 state + settings -> what Discord receives
      text.rs                 file and agent labels, UTF-8 truncation
      icons.rs                language and harness asset catalogue
      buttons.rs              buttons and Discord's limits
  adapters/                   the outside world
    krad/                     host SDK: hooks, settings, conversions
      convert.rs              SDK types -> domain types
      settings.rs             SettingsValues -> Settings
      titlebar.rs             sends the button state to the host
    discord/                  thread that owns the Discord IPC client
      limiter.rs              debounce, rate window and reconnect backoff
      activity.rs             PresenceView -> Discord activity
    git/                      branch and origin read from .git
  app/                        orchestration
    monitor.rs                timer thread: Git refresh and idle
```

Each module keeps its tests in a sibling `tests.rs` when they outgrow the file.
`krad-extension-sdk` is pinned to the Git commit `9c0812c` (release 0.4.0) because the 0.4.0 crate on crates.io predates the app event API. Switch back to the crates.io version in `Cargo.toml` once a release includes it.

### Releasing

1. Bump `version` in `Cargo.toml`, refresh the lock file and commit the bump on `main`:

   ```sh
   cargo check
   git commit -am "Bump version to 0.2.0"
   ```

2. Tag that commit and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

   Pushing a `v*` tag publishes the release for real.

3. The **Release** workflow (`.github/workflows/release.yml`) then tests and builds `darwin_arm64`, `darwin_x64`, `linux_x64`, `linux_arm64`, `win_x64` and `win_arm64` on GitHub-hosted runners, creates the GitHub Release with `krad-discord-rpc-<os>-<arch>[.exe]` assets, and rewrites the binaries block and `version` in `extension.toml` with `scripts/binaries.py release` before pushing that commit to `main`. The two cross-compiled targets (`darwin_x64`, `win_arm64`) only build.

To redo the manifest step by hand, put the binaries in `dist/` under those names and run:

```sh
python3 scripts/binaries.py release 0.2.0 dist   # url + sha256 for tag v0.2.0
python3 scripts/binaries.py local                # back to bin/ for development
```

`scripts/binaries.py` only touches the block between the `BEGIN BINARIES` and `END BINARIES` markers and the top-level `version` line.

## License

[MIT](LICENSE). Brand icons in `assets/discord/` come from
[Simple Icons](https://simpleicons.org) (CC0); the brands belong to their owners.
