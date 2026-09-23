# acm (autocommit-cli)

Generate a Conventional Commit message for your **staged** changes with the
Gemini free tier.

Gemini only writes the message text. Everything else is deterministic Rust:
reading and filtering the diff (lockfiles, binaries, and generated files are
listed but not sent), truncating it, inferring the type/scope from paths,
validating the reply, asking once for a correction, and repairing it if the
model still gets it wrong. You never get an invalid or empty message.

## Install

```sh
cargo install autocommit-cli      # from crates.io; installs the `acm` binary
# or, from a checkout:
cargo install --path .
```

## Configure

```sh
acm --init        # creates ~/.config/autocommit/config.yaml (mode 600)
$EDITOR ~/.config/autocommit/config.yaml
```

```yaml
api_key: "AIza..."              # free key: https://aistudio.google.com/apikey
# model: gemini-3.1-flash-lite
# base_url: https://generativelanguage.googleapis.com
```

Precedence: `--model` flag > environment variable > config file > default.

| Setting | Env var | Config key | Default |
|---|---|---|---|
| API key | `GEMINI_API_KEY` | `api_key` | (required) |
| Model | `AUTOCOMMIT_MODEL` | `model` | `gemini-3.1-flash-lite` |
| API base URL | `GEMINI_BASE_URL` | `base_url` | `https://generativelanguage.googleapis.com` |
| Config path | `AUTOCOMMIT_CONFIG` | | `$XDG_CONFIG_HOME/autocommit/config.yaml`, else `~/.config/autocommit/config.yaml` |
| Retry delay | `AUTOCOMMIT_RETRY_DELAY_MS` | | `2000` |

Unknown keys in the config file are an error, so a typo such as `apikey:` fails
loudly instead of being ignored. If the file is readable by other users, acm
prints a warning.

## Use

```sh
git add -p
acm               # show message, then [y]es / [e]dit / [r]egenerate / [n]o
acm --yes         # commit immediately
acm --dry-run     # print only
acm -m gemini-3.5-flash
```

## Message rules

`type(scope): description`. `type` is one of feat, fix, docs, style, refactor,
perf, test, build, ci, chore, revert. The subject line is at most 72
characters, starts with a lowercase letter, and has no trailing period. The
body is optional and holds at most 3 `- ` bullets.

When all changed files are docs, tests, CI, or manifests, the type is locked to
`docs`, `test`, `ci`, or `chore` in code, whatever the model says.

## Develop

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

Tests never touch the real API or your real config. They use a local mock
server, a temporary git repo, and `AUTOCOMMIT_CONFIG` pointed at a temp file.

## License

MIT OR Apache-2.0
