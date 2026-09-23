# autocommit — implementation plan

Order = dependency order. Each task: write failing tests first (RED), then code
(GREEN), then refactor. Pure modules get unit tests in-file; end-to-end
behaviour lives in `tests/cli.rs`.

| # | File | Responsibility | Tests |
|---|------|----------------|-------|
| 1 | `Cargo.toml` | deps: clap(derive), serde(derive), serde_json, anyhow, ureq 3; dev: tempfile | build |
| 2 | `src/diff.rs` | types `FileChange {path, status, added, removed, binary, patch}`; parse name-status/numstat/patch; `filter` (lock/binary/generated); `render(files, limits) -> String` (stat summary + truncated patches) | unit: parsing, filtering, per-file + total truncation, omitted marker |
| 3 | `src/hints.rs` | `Hints {locked_type, suggested_type, scope}` from `&[FileChange]` | unit: each lock rule, mixed → none, all-added → feat, scope rules incl. src/x.rs, <60% → none |
| 4 | `src/message.rs` | `sanitize(raw)`, `validate(msg) -> Result<(), Violation>`, `enforce_type`, `repair(msg, hints)` | unit: fences/quotes/labels stripped, each violation, repair outputs always valid |
| 5 | `src/prompt.rs` | `build(hints, rendered_diff)`, `correction(prev, violation)` | unit: hints appear, locked type phrased as MUST |
| 6 | `src/gemini.rs` | `Client {base_url, model, api_key, retry_delay}`; `generate(prompt) -> Result<String>`; retry once on 429/5xx | via integration mock server |
| 7 | `src/git.rs` | `staged_changes() -> Vec<FileChange>`, `commit(message)` | via integration (temp repo) |
| 8 | `src/app.rs` | pipeline: collect → filter → hints → prompt → generate → sanitize/validate → corrective retry → repair | integration |
| 9 | `src/main.rs` + `src/cli.rs` | clap args, env config, TTY check, interactive loop (y/e/r/n), `$EDITOR` | integration for --dry-run/--yes/errors |
| 10 | `tests/cli.rs` | temp git repo + `std::net::TcpListener` mock Gemini (scripted responses, records requests); runs built binary with `GEMINI_BASE_URL`, `AUTOCOMMIT_RETRY_DELAY_MS=0` | spec "Done when" cases |
| 11 | `README.md` | install, env, usage | — |

Then: fresh-context review subagent (correctness + security), fix findings,
full gate run, optional real-Gemini smoke test.
