# acm (autocommit-cli)

Generate a Conventional Commit message for your **staged** changes with the
Gemini free tier. Works on Linux, macOS, and Windows.

Gemini only writes the message text. Everything else is deterministic Rust:
reading and filtering the diff (lockfiles, binaries, and generated files are
listed but not sent), truncating it, inferring the type/scope from paths,
validating the reply, asking once for a correction, and repairing it if the
model still gets it wrong. You never get an invalid or empty message.

## Requirements

- [git](https://git-scm.com/downloads) on your `PATH`
- [Rust](https://rustup.rs) 1.89 or newer (to build it with `cargo install`)
- A free Gemini API key: <https://aistudio.google.com/apikey>

## Install

### Linux / macOS

```sh
# macOS only, once: the C compiler used by the TLS library
xcode-select --install

cargo install autocommit-cli     # installs the `acm` command
```

### Windows

1. Install **Git for Windows**: <https://git-scm.com/download/win>
   (or `winget install --id Git.Git -e`).
2. Install **Rust** with `rustup-init.exe` from <https://rustup.rs>
   (or `winget install --id Rustlang.Rustup -e`). When it asks, let it install
   the **Visual Studio C++ Build Tools**. They are required to compile Rust
   programs on Windows (MSVC linker and C compiler).
3. Open a **new** PowerShell window, so `cargo` is on `PATH`, and run:

   ```powershell
   cargo install autocommit-cli
   acm --version
   ```

`acm` is installed to `%USERPROFILE%\.cargo\bin`, which rustup adds to `PATH`.
It works in PowerShell, cmd, Windows Terminal, and Git Bash.

### From a checkout

```sh
cargo install --path .
```

## Configure

```sh
acm --init        # creates the config file (private, mode 600 on Unix)
```

Then put your key in the file:

| OS | Config file |
|---|---|
| Linux / macOS | `~/.config/autocommit/config.yaml` (or `$XDG_CONFIG_HOME/autocommit/config.yaml`) |
| Windows | `%APPDATA%\autocommit\config.yaml`, e.g. `C:\Users\you\AppData\Roaming\autocommit\config.yaml` |

```yaml
api_key: "AIza..."              # free key: https://aistudio.google.com/apikey
# model: gemini-3.1-flash-lite
# base_url: https://generativelanguage.googleapis.com
```

On Windows, open the file with:

```powershell
notepad "$env:APPDATA\autocommit\config.yaml"
```

The Windows path is the same whether you run `acm` from PowerShell, cmd, or
Git Bash.

### Environment variables instead of a file

Linux / macOS:

```sh
export GEMINI_API_KEY="AIza..."           # add to ~/.bashrc or ~/.zshrc
```

Windows (PowerShell). This stores the key for your user; open a new window
afterwards:

```powershell
setx GEMINI_API_KEY "AIza..."
# only for the current window:
$env:GEMINI_API_KEY = "AIza..."
```

Precedence: `--model` flag > environment variable > config file > default.

| Setting | Env var | Config key | Default |
|---|---|---|---|
| API key | `GEMINI_API_KEY` | `api_key` | (required) |
| Model | `AUTOCOMMIT_MODEL` | `model` | `gemini-3.1-flash-lite` |
| API base URL | `GEMINI_BASE_URL` | `base_url` | `https://generativelanguage.googleapis.com` |
| Config path | `AUTOCOMMIT_CONFIG` | | see the table above |
| Retry delay | `AUTOCOMMIT_RETRY_DELAY_MS` | | `2000` |

Unknown keys in the config file are an error, so a typo such as `apikey:` fails
loudly instead of being ignored. On Linux and macOS, acm warns if the file is
readable by other users.

## Use

```sh
git add -p
acm               # show message, then [y]es / [e]dit / [r]egenerate / [n]o
acm --yes         # commit immediately
acm --edit        # open the message in git's editor, then commit
acm --dry-run     # print only
acm -m gemini-3.5-flash
```

**Editing** (`e` or `--edit`) opens the message in the same editor `git commit`
uses. To choose one, set `core.editor` in git:

```sh
git config --global core.editor "code --wait"      # VS Code (any OS)
git config --global core.editor "nano"             # Linux / macOS
git config --global core.editor notepad            # Windows
```

Save and close the editor to commit. Delete every line and save to cancel.

## Message rules

`type(scope): description`. `type` is one of feat, fix, docs, style, refactor,
perf, test, build, ci, chore, revert. The subject line is at most 72
characters, starts with a lowercase letter, and has no trailing period. The
body is optional and holds at most 3 `- ` bullets.

When all changed files are docs, tests, CI, or manifests, the type is locked to
`docs`, `test`, `ci`, or `chore` in code, whatever the model says.

## Troubleshooting

| Problem | Fix |
|---|---|
| `acm: command not found` / `'acm' is not recognized` | Open a new terminal. Check that `~/.cargo/bin` (Windows: `%USERPROFILE%\.cargo\bin`) is on `PATH`. |
| Windows: `link.exe not found` during install | Install "Desktop development with C++" from the Visual Studio Build Tools, then retry. |
| macOS: `xcrun: error` during install | Run `xcode-select --install`. |
| `failed to run git` | Install git and make sure `git --version` works in the same terminal. |
| `no Gemini API key` | Run `acm --init` and fill in `api_key`, or set `GEMINI_API_KEY`. |
| `HTTP 429` | Free-tier rate limit. Wait a minute, or try another model with `-m`. |
| `HTTP 404` for the model | The model was retired. Pick a current one from <https://ai.google.dev/gemini-api/docs/models>. |

## Develop

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

Tests never touch the real API or your real config. They use a local mock
server, a temporary git repo, and `AUTOCOMMIT_CONFIG` pointed at a temp file.
CI runs them on Linux, macOS, and Windows (`.github/workflows/ci.yml`).

## License

MIT OR Apache-2.0
