# AGENTS.md — autocommit

Rust CLI that generates Conventional Commit messages for staged changes via
Gemini (free tier). Spec: `docs/spec.md`. Plan: `docs/plan.md`.

## Commands
- build: `cargo build`
- test: `cargo test`
- lint: `cargo clippy --all-targets -- -D warnings`
- format: `cargo fmt` (check: `cargo fmt --check`)

## Design rules
- Gemini ONLY writes message text. All deterministic logic (diff filtering,
  truncation, type/scope hints, validation, repair, git) stays in Rust.
- Treat LLM output as untrusted: always sanitize + validate before use.
- Keep pure logic (diff/hints/message/prompt) free of I/O so it is unit-testable.
- Git access via `std::process::Command` calling `git`; no git2.
- HTTP via blocking `ureq`; no async runtime.
- Tests must never hit the real Gemini API; use `GEMINI_BASE_URL` + the mock
  server in `tests/`.
- Never print or log the API key.

## Definition of done
build + test + clippy (-D warnings) + fmt --check all green; new behaviour
covered by a test that was seen failing first.
