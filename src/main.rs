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
use std::process::ExitCode;
use std::time::Duration;

/// Generate a Conventional Commit message for the staged changes using Gemini.
#[derive(Parser, Debug)]
#[command(name = "acm", version, about)]
struct Cli {
    /// Commit immediately without asking.
    #[arg(short, long, conflicts_with_all = ["dry_run", "edit"])]
    yes: bool,

    /// Only print the generated message; never commit.
    #[arg(short = 'n', long, conflicts_with = "edit")]
    dry_run: bool,

    /// Open the generated message in git's editor, then commit.
    #[arg(short, long)]
    edit: bool,

    /// Gemini model name [env: AUTOCOMMIT_MODEL] [default: gemini-3.1-flash-lite].
    #[arg(short, long)]
    model: Option<String>,

    /// Create a config file template and exit (see README for its location).
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
        let path = config_path.context(
            "cannot determine the config path (set HOME, or APPDATA on Windows, or AUTOCOMMIT_CONFIG)",
        )?;
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
    let interactive = !cli.yes && !cli.dry_run && !cli.edit;
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
    if cli.yes || cli.edit {
        return git::commit(&msg, cli.edit);
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
            "y" | "yes" | "" => return git::commit(&msg, false),
            "e" | "edit" => return git::commit(&msg, true),
            "r" | "regenerate" => msg = generator.generate()?,
            "n" | "no" | "q" => {
                eprintln!("aborted, nothing committed");
                return Ok(());
            }
            other => eprintln!("unknown answer {other:?}"),
        }
    }
}
