# autocommit — spec

## Intent
A Rust CLI that writes a git commit message for the currently STAGED changes
using the Gemini free tier. Gemini is weak, so it is used ONLY to write the
message text. Everything deterministic is done in Rust: reading/filtering/
truncating the diff, inferring type/scope hints, building the prompt,
validating + repairing the output, and running git. Gemini output is treated
as untrusted input.

## Stack
Rust (edition 2024), git CLI via `std::process::Command`.
Crates: clap (derive), serde, serde_json, anyhow, ureq 3 (rustls, blocking).
Dev: tempfile. No tokio, no git2.

## Usage
```
autocommit [--yes] [--dry-run] [--model <name>]
```
- Reads only staged changes (`git diff --cached`). Never runs `git add`.
- Default (interactive TTY): print message, prompt
  `[y]es commit / [e]dit / [r]egenerate / [n]o`.
  - `e` opens `$VISUAL` / `$EDITOR` (fallback `vi`) on a temp file.
- `--yes`: commit without prompting.
- `--dry-run`: print the message to stdout, never commit.
- Non-TTY stdin without `--yes`/`--dry-run`: error (no silent hang).
- Commit via `git commit -F <tmpfile>` (hooks still run).

## Config (flag > env > ~/.config/autocommit/config.yaml > default)
- `GEMINI_API_KEY` (required unless no API call is made)
- `AUTOCOMMIT_MODEL` (default `gemini-3.1-flash-lite`; `--model` wins)
- `GEMINI_BASE_URL` (default `https://generativelanguage.googleapis.com`;
  used by tests to point at a mock server)

## Message format (Conventional Commits, English)
```
<type>(<scope>)?: <description>

- optional bullet (max 3)
```
- type ∈ feat fix docs style refactor perf test build ci chore revert
- subject line ≤ 72 chars, description starts lowercase, no trailing period.

## Pipeline
1. **Collect**: `git diff --cached --name-status`, `--numstat`, full patch.
   No staged changes → error, exit 1.
2. **Filter**: drop lockfiles (Cargo.lock, package-lock.json, yarn.lock,
   pnpm-lock.yaml, poetry.lock, uv.lock, go.sum, composer.lock, Gemfile.lock),
   binary files, generated/minified (`*.min.js`, `*.min.css`, `*.map`,
   `dist/`, `target/`). Dropped files are still listed in the stat summary.
3. **Truncate**: ≤150 patch lines per file, ≤12 000 chars total; overflow
   files appear only in the stat summary, marked `(diff omitted)`.
4. **Hints** (deterministic):
   - all files docs (`*.md`, `*.rst`, `*.txt`, `docs/`) → LOCK `docs`
   - all files tests (`tests/`, `test/`, `*_test.*`, `*.test.*`, `*.spec.*`,
     `test_*.py`) → LOCK `test`
   - all files CI (`.github/workflows/`, `.gitlab-ci.yml`) → LOCK `ci`
   - all files manifests/lockfiles/tool config → LOCK `chore`
   - otherwise no lock; if all files are newly added → suggest `feat`.
   - scope: dominant module (first dir; under `src/` use next component, file
     stem for `src/x.rs`) owning ≥60% of changed lines; else no scope.
5. **Prompt**: short, strict instructions + 2 examples + hints + stat + diff.
   `temperature 0.2`.
6. **Gemini**: `POST {base}/v1beta/models/{model}:generateContent`,
   key in `x-goog-api-key` header. 429/5xx/transport → retry once after
   backoff (2s; tests use 0). Other errors / second failure → clear error,
   exit non-zero. Empty candidate text → error.
7. **Validate/repair**: sanitize (strip code fences, surrounding quotes,
   leading `Commit message:`-style labels, blank lines), force locked type.
   Invalid → one corrective re-ask stating the violation. Still invalid →
   deterministic repair (lowercase, strip period, truncate at word boundary
   to 72, fall back to hinted/`chore` type). Never commit an empty message.

## Out of scope (v1)
git hook mode (prepare-commit-msg), auto `git add`, other LLM providers,
config file, multi-language messages, streaming.

## Done when
- `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt --check` all pass.
- Integration tests (temp git repo + local mock Gemini server) prove:
  dry-run prints a valid message and does not commit; `--yes` creates a commit
  with that message; no staged changes → exit 1; 429 then 200 → success;
  invalid output → corrective retry; locked type is enforced; API key missing
  → clear error.
- Fresh-context review found no correctness issues left unfixed.
- Manual smoke run against real Gemini (if the user provides a key).
