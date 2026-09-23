mod app;
mod config;
mod diff;
mod gemini;
mod git;
mod hints;
mod message;
mod prompt;

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::io::{BufRead, IsTerminal, Write};
use std::process::{Command, ExitCode};
use std::time::Duration;

/// Generate a Conventional Commit message for the staged changes using Gemini.
#[derive(Parser, Debug)]
#[command(name = "acm", version, about)]
struct Cli {
    /// Commit immediately without asking.
    #[arg(short, long, conflicts_with = "dry_run")]
    yes: bool,

    /// Only print the generated message; never commit.
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Gemini model name [env: AUTOCOMMIT_MODEL] [default: gemini-3.1-flash-lite].
    #[arg(short, long)]
    model: Option<String>,

    /// Create a config file template (~/.config/autocommit/config.yaml) and exit.
    #[arg(long, exclusive = true)]
    init: bool,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("acm: error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let env = |k: &str| std::env::var(k).ok();
    let config_path = config::default_path(env);
    if cli.init {
        let path = config_path.context("cannot determine the config path (HOME is not set)")?;
        config::init(&path)?;
        eprintln!("created {}; put your api_key there", path.display());
        return Ok(());
    }

    git::ensure_repo()?;
    let files = git::staged_changes()?;
    if files.is_empty() {
        bail!("no staged changes (stage files with `git add` first)");
    }
    if files.iter().any(|f| f.status == diff::Status::Unmerged) {
        bail!("the index has unmerged paths; resolve the conflicts and `git add` them first");
    }
    let interactive = !cli.yes && !cli.dry_run;
    if interactive && !std::io::stdin().is_terminal() {
        bail!("stdin is not a terminal; use --yes to commit or --dry-run to print");
    }

    let file = match &config_path {
        Some(p) => config::FileConfig::load(p)?,
        None => config::FileConfig::default(),
    };
    let cfg = config::Config::resolve(cli.model.as_deref(), &file, env)?;
    let retry_ms = std::env::var("AUTOCOMMIT_RETRY_DELAY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let client = gemini::Client::new(
        cfg.base_url,
        cfg.model,
        cfg.api_key,
        Duration::from_millis(retry_ms),
    );
    let generator = app::Generator::new(&client, &files);

    let mut msg = generator.generate()?;
    if cli.dry_run {
        println!("{msg}");
        return Ok(());
    }
    if cli.yes {
        return git::commit(&msg);
    }

    loop {
        eprintln!("\n{msg}\n");
        eprint!("Commit? [y]es / [e]dit / [r]egenerate / [n]o: ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        if std::io::stdin().lock().read_line(&mut answer)? == 0 {
            bail!("aborted");
        }
        match answer.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" | "" => return git::commit(&msg),
            "e" | "edit" => {
                let edited = edit(&msg)?;
                if edited.trim().is_empty() {
                    eprintln!("empty message, keeping the previous one");
                } else {
                    msg = edited;
                }
            }
            "r" | "regenerate" => msg = generator.generate()?,
            "n" | "no" | "q" => {
                eprintln!("aborted, nothing committed");
                return Ok(());
            }
            other => eprintln!("unknown answer {other:?}"),
        }
    }
}

/// Open `$VISUAL` / `$EDITOR` (fallback `vi`) on the message.
fn edit(msg: &str) -> Result<String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    // Private (0700), unpredictable dir: no symlink planting in /tmp.
    let dir = tempfile::Builder::new()
        .prefix("autocommit-")
        .tempdir()
        .context("failed to create a temp dir for the editor")?;
    let path = dir.path().join("COMMIT_EDITMSG");
    std::fs::write(&path, format!("{msg}\n"))?;
    // Run through the shell so EDITOR="code --wait" works.
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(&path)
        .status()
        .with_context(|| format!("failed to start editor {editor:?}"))?;
    if !status.success() {
        bail!("editor exited with {status}");
    }
    Ok(std::fs::read_to_string(&path)?
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string())
}
