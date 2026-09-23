# autocommit

Generate a Conventional Commit message for your **staged** changes with the
Gemini free tier.

Gemini only writes the message text. Everything else is deterministic Rust:
reading and filtering the diff (lockfiles, binaries, and generated files are
listed but not sent), truncating it, inferring the type/scope from paths,
validating the reply, asking once for a correction, and repairing it if the
model still gets it wrong. You never get an invalid or empty message.

## Install

```sh
cargo install --path .
export GEMINI_API_KEY=...   # free key: https://aistudio.google.com/apikey
```

## Use

```sh
git add -p
autocommit            # show message, then [y]es / [e]dit / [r]egenerate / [n]o
autocommit --yes      # commit immediately
autocommit --dry-run  # print only
autocommit -m gemini-3.5-flash
```

| Env var | Default | |
|---|---|---|
| `GEMINI_API_KEY` | (required) | sent as the `x-goog-api-key` header |
| `AUTOCOMMIT_MODEL` | `gemini-3.1-flash-lite` | `--model` overrides it |
| `GEMINI_BASE_URL` | `https://generativelanguage.googleapis.com` | |
| `AUTOCOMMIT_RETRY_DELAY_MS` | `2000` | wait before the single retry on 429/5xx |

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

Tests never touch the real API. They use a local mock server and a temporary
git repo.
