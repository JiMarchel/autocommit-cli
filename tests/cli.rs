//! End-to-end tests: real git in a temp repo, scripted local mock of the Gemini API.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;

const BIN: &str = env!("CARGO_BIN_EXE_acm");

/// An empty global git config (portable replacement for `/dev/null`).
fn empty_gitconfig() -> &'static Path {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let p = std::env::temp_dir().join(format!("acm-test-gitconfig-{}", std::process::id()));
        std::fs::write(&p, "").unwrap();
        p
    })
}

/// Base command for the binary, isolated from the developer's real config.
fn acm(dir: &Path) -> Command {
    let mut cmd = Command::new(BIN);
    cmd.current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", empty_gitconfig())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("AUTOCOMMIT_RETRY_DELAY_MS", "0")
        .env("AUTOCOMMIT_CONFIG", dir.join(".no-such-config.yaml"))
        .env_remove("GEMINI_API_KEY")
        .env_remove("GEMINI_BASE_URL")
        .env_remove("AUTOCOMMIT_MODEL");
    cmd
}

#[derive(Debug, Clone)]
struct Recorded {
    request_line: String,
    headers: Vec<(String, String)>,
    body: serde_json::Value,
}

impl Recorded {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn prompt(&self) -> &str {
        self.body["contents"][0]["parts"][0]["text"]
            .as_str()
            .unwrap_or_default()
    }
}

struct MockGemini {
    url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl MockGemini {
    /// Serve the given (status, body) responses in order, one per connection.
    fn start(responses: Vec<(u16, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        thread::spawn(move || {
            for (status, body) in responses {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream);
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut headers = Vec::new();
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    let (k, v) = line.split_once(':').unwrap();
                    if k.eq_ignore_ascii_case("content-length") {
                        len = v.trim().parse().unwrap();
                    }
                    headers.push((k.trim().to_string(), v.trim().to_string()));
                }
                let mut buf = vec![0; len];
                reader.read_exact(&mut buf).unwrap();
                log.lock().unwrap().push(Recorded {
                    request_line: request_line.trim_end().to_string(),
                    headers,
                    body: serde_json::from_slice(&buf).unwrap_or_default(),
                });
                let mut stream = reader.into_inner();
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        MockGemini { url, requests }
    }

    fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

fn ok(text: &str) -> (u16, String) {
    let body = serde_json::json!({
        "candidates": [{"content": {"role": "model", "parts": [{"text": text}]}}]
    });
    (200, body.to_string())
}

fn err(status: u16, message: &str) -> (u16, String) {
    let body = serde_json::json!({"error": {"code": status, "message": message, "status": "X"}});
    (status, body.to_string())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", empty_gitconfig())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    dir
}

fn stage(dir: &Path, path: &str, content: &str) {
    let full = dir.join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, content).unwrap();
    git(dir, &["add", path]);
}

fn run(dir: &Path, mock: Option<&MockGemini>, args: &[&str]) -> Output {
    run_with(dir, mock, args, &[])
}

fn run_with(dir: &Path, mock: Option<&MockGemini>, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = acm(dir);
    cmd.args(args).env("GEMINI_API_KEY", "test-key-123");
    cmd.envs(envs.iter().copied());
    match mock {
        Some(m) => cmd.env("GEMINI_BASE_URL", &m.url),
        // Nothing listens here: any request fails fast.
        None => cmd.env("GEMINI_BASE_URL", "http://127.0.0.1:9"),
    };
    cmd.output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn commit_count(dir: &Path) -> usize {
    let out = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    if out.status.success() {
        String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
    } else {
        0
    }
}

#[test]
fn dry_run_prints_message_and_does_not_commit() {
    let dir = repo();
    stage(dir.path(), "src/parser.rs", "pub fn parse() {}\n");
    let mock = MockGemini::start(vec![ok("feat(parser): add parse function")]);

    let out = run(dir.path(), Some(&mock), &["--dry-run"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "feat(parser): add parse function");
    assert_eq!(commit_count(dir.path()), 0);
}

#[test]
fn sends_expected_request() {
    let dir = repo();
    stage(dir.path(), "src/parser.rs", "pub fn parse() {}\n");
    stage(
        dir.path(),
        "Cargo.lock",
        "[[package]]\nname = \"secret-lock-content\"\n",
    );
    let mock = MockGemini::start(vec![ok("feat(parser): add parse function")]);

    let out = run(
        dir.path(),
        Some(&mock),
        &["--dry-run", "--model", "my-model"],
    );
    assert!(out.status.success(), "{}", stderr(&out));

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    let r = &reqs[0];
    assert_eq!(
        r.request_line,
        "POST /v1beta/models/my-model:generateContent HTTP/1.1"
    );
    assert_eq!(r.header("x-goog-api-key"), Some("test-key-123"));
    assert!(!r.request_line.contains("test-key-123"));
    let prompt = r.prompt();
    assert!(prompt.contains("pub fn parse() {}"));
    assert!(prompt.contains("Cargo.lock"));
    assert!(prompt.contains("[diff omitted: lockfile]"));
    assert!(!prompt.contains("secret-lock-content"));
    assert!(!stderr(&out).contains("test-key-123"));
}

#[test]
fn yes_commits_with_generated_message() {
    let dir = repo();
    stage(dir.path(), "src/parser.rs", "pub fn parse() {}\n");
    let mock = MockGemini::start(vec![ok(
        "```\nfeat(parser): add parse function\n\n- expose parse()\n```",
    )]);

    let out = run(dir.path(), Some(&mock), &["--yes"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(commit_count(dir.path()), 1);
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%B"]).trim(),
        "feat(parser): add parse function\n\n- expose parse()"
    );
}

#[test]
fn no_staged_changes_fails_without_calling_api() {
    let dir = repo();
    std::fs::write(dir.path().join("untracked.rs"), "x").unwrap();

    let out = run(dir.path(), None, &["--dry-run"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("no staged changes"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn outside_git_repo_fails() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), None, &["--dry-run"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("git"), "{}", stderr(&out));
}

#[test]
fn missing_api_key_is_a_clear_error() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let out = acm(dir.path())
        .arg("--dry-run")
        .env("GEMINI_BASE_URL", "http://127.0.0.1:9")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("GEMINI_API_KEY"), "{}", stderr(&out));
}

fn write_config(dir: &Path, text: &str, mode: u32) -> std::path::PathBuf {
    let path = dir.join("cfg").join("config.yaml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = mode;
    path
}

#[test]
fn config_file_supplies_key_model_and_url() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![ok("fix: handle x")]);
    let cfg = tempfile::tempdir().unwrap();
    let path = write_config(
        cfg.path(),
        &format!(
            "api_key: file-key-456\nmodel: file-model\nbase_url: {}\n",
            mock.url
        ),
        0o600,
    );

    let out = acm(dir.path())
        .arg("--dry-run")
        .env("AUTOCOMMIT_CONFIG", &path)
        .output()
        .unwrap();

    assert!(out.status.success(), "{}", stderr(&out));
    let r = &mock.requests()[0];
    assert_eq!(r.header("x-goog-api-key"), Some("file-key-456"));
    assert!(
        r.request_line.contains("/models/file-model:"),
        "{}",
        r.request_line
    );
    assert!(!stderr(&out).contains("readable"), "{}", stderr(&out));
}

#[test]
fn env_and_flag_override_config_file() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![ok("fix: handle x")]);
    let cfg = tempfile::tempdir().unwrap();
    let path = write_config(
        cfg.path(),
        "api_key: file-key\nmodel: file-model\nbase_url: http://127.0.0.1:9\n",
        0o600,
    );

    let out = acm(dir.path())
        .args(["--dry-run", "--model", "flag-model"])
        .env("AUTOCOMMIT_CONFIG", &path)
        .env("GEMINI_API_KEY", "env-key")
        .env("GEMINI_BASE_URL", &mock.url)
        .output()
        .unwrap();

    assert!(out.status.success(), "{}", stderr(&out));
    let r = &mock.requests()[0];
    assert_eq!(r.header("x-goog-api-key"), Some("env-key"));
    assert!(
        r.request_line.contains("/models/flag-model:"),
        "{}",
        r.request_line
    );
}

#[cfg(unix)]
#[test]
fn world_readable_config_warns() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![ok("fix: handle x")]);
    let cfg = tempfile::tempdir().unwrap();
    let path = write_config(
        cfg.path(),
        &format!("api_key: file-key\nbase_url: {}\n", mock.url),
        0o644,
    );

    let out = acm(dir.path())
        .arg("--dry-run")
        .env("AUTOCOMMIT_CONFIG", &path)
        .output()
        .unwrap();

    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("chmod 600"), "{}", stderr(&out));
    assert!(!stderr(&out).contains("file-key"));
}

#[test]
fn invalid_config_is_a_clear_error() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let cfg = tempfile::tempdir().unwrap();
    let path = write_config(cfg.path(), "apikey: oops\n", 0o600);

    let out = acm(dir.path())
        .arg("--dry-run")
        .env("AUTOCOMMIT_CONFIG", &path)
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("config.yaml") && err.contains("apikey"),
        "{err}"
    );
}

#[test]
fn init_writes_private_config_template() {
    let cfg = tempfile::tempdir().unwrap();
    let path = cfg.path().join("sub").join("config.yaml");

    let out = acm(cfg.path())
        .arg("--init")
        .env("AUTOCOMMIT_CONFIG", &path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("api_key:"), "{text}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    // Never overwrite an existing config.
    std::fs::write(&path, "api_key: mine\n").unwrap();
    let out = acm(cfg.path())
        .arg("--init")
        .env("AUTOCOMMIT_CONFIG", &path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "api_key: mine\n");
}

#[test]
fn retries_once_on_rate_limit() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![err(429, "quota"), ok("fix: handle x")]);

    let out = run(dir.path(), Some(&mock), &["--dry-run"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "fix: handle x");
    assert_eq!(mock.requests().len(), 2);
}

#[test]
fn gives_up_after_second_server_error() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![err(503, "overloaded"), err(503, "still overloaded")]);

    let out = run(dir.path(), Some(&mock), &["--yes"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("503"), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("still overloaded"),
        "{}",
        stderr(&out)
    );
    assert_eq!(commit_count(dir.path()), 0);
}

#[test]
fn does_not_retry_client_errors() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![err(400, "API key not valid"), ok("fix: never used")]);

    let out = run(dir.path(), Some(&mock), &["--dry-run"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("API key not valid"),
        "{}",
        stderr(&out)
    );
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn empty_model_reply_is_an_error() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let body = serde_json::json!({"candidates": [{"finishReason": "SAFETY"}]}).to_string();
    let mock = MockGemini::start(vec![(200, body)]);

    let out = run(dir.path(), Some(&mock), &["--dry-run"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("SAFETY"), "{}", stderr(&out));
}

#[test]
fn invalid_reply_triggers_one_correction() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![ok("Added the x thing."), ok("feat: add x thing")]);

    let out = run(dir.path(), Some(&mock), &["--dry-run"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "feat: add x thing");
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[1].prompt().contains("Added the x thing."));
    assert!(reqs[1].prompt().contains("invalid because"));
}

#[test]
fn still_invalid_after_correction_is_repaired() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![
        ok("Added the x thing."),
        ok("Feature: Added the x thing."),
    ]);

    let out = run(dir.path(), Some(&mock), &["--dry-run"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "feat: added the x thing");
    assert_eq!(mock.requests().len(), 2);
}

#[test]
fn locked_type_is_enforced_without_extra_request() {
    let dir = repo();
    stage(dir.path(), "README.md", "# hello\n");
    let mock = MockGemini::start(vec![ok("feat: add readme title")]);

    let out = run(dir.path(), Some(&mock), &["--dry-run"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "docs: add readme title");
    assert_eq!(mock.requests().len(), 1);
    assert!(mock.requests()[0].prompt().contains("MUST be \"docs\""));
}

#[cfg(unix)]
#[test]
fn non_ascii_paths_and_prefix_configs_keep_their_diff() {
    let dir = repo();
    stage(dir.path(), "café.txt", "v1\n");
    stage(dir.path(), "x", "plain\n");
    stage(dir.path(), "other.rs", "v1\n");
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    git(dir.path(), &["config", "diff.noprefix", "true"]);
    stage(dir.path(), "café.txt", "v2-CAFE\n");
    stage(dir.path(), "other.rs", "v2-OTHER\n");
    // typechange: regular file -> symlink (two patch blocks for one entry)
    std::fs::remove_file(dir.path().join("x")).unwrap();
    std::os::unix::fs::symlink("somewhere", dir.path().join("x")).unwrap();
    git(dir.path(), &["add", "x"]);
    let mock = MockGemini::start(vec![ok("fix: update files")]);

    let out = run(dir.path(), Some(&mock), &["--dry-run"]);

    assert!(out.status.success(), "{}", stderr(&out));
    let prompt = mock.requests()[0].prompt().to_string();
    assert!(prompt.contains("v2-CAFE"), "{prompt}");
    assert!(prompt.contains("v2-OTHER"), "{prompt}");
    assert!(prompt.contains("+somewhere"), "{prompt}");
}

#[test]
fn unmerged_index_fails_without_calling_api() {
    let dir = repo();
    stage(dir.path(), "c.txt", "base\n");
    git(dir.path(), &["commit", "-q", "-m", "base"]);
    git(dir.path(), &["checkout", "-q", "-b", "other"]);
    stage(dir.path(), "c.txt", "other\n");
    git(dir.path(), &["commit", "-q", "-m", "other"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    stage(dir.path(), "c.txt", "main\n");
    git(dir.path(), &["commit", "-q", "-m", "main"]);
    let _ = Command::new("git")
        .args(["merge", "-q", "other"])
        .current_dir(dir.path())
        .env("GIT_CONFIG_GLOBAL", empty_gitconfig())
        .output()
        .unwrap();

    // Nothing listens on the default URL in `run(.., None, ..)`, so an API call would error differently.
    let out = run(dir.path(), None, &["--dry-run"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("unmerged"), "{}", stderr(&out));
}

#[test]
fn edit_flag_opens_git_editor_then_commits() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![ok("fix: handle x")]);

    // `-i.bak` works with both GNU sed and BSD/macOS sed; Git for Windows ships sed.
    let out = run_with(
        dir.path(),
        Some(&mock),
        &["--edit"],
        &[("GIT_EDITOR", "sed -i.bak -e s/handle/rework/")],
    );

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%B"]).trim(),
        "fix: rework x"
    );
}

#[test]
fn emptied_message_in_editor_aborts_commit() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    let mock = MockGemini::start(vec![ok("fix: handle x")]);

    let out = run_with(
        dir.path(),
        Some(&mock),
        &["--edit"],
        &[("GIT_EDITOR", "sed -i.bak -e d")],
    );

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("aborted"), "{}", stderr(&out));
    assert_eq!(commit_count(dir.path()), 0);
}

#[test]
fn non_interactive_without_flags_refuses() {
    let dir = repo();
    stage(dir.path(), "src/a.rs", "x\n");
    // `Command::output` gives the child a null stdin, i.e. not a TTY.
    let out = run(dir.path(), None, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("--yes"), "{}", stderr(&out));
    assert_eq!(commit_count(dir.path()), 0);
}
