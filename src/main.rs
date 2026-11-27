//! rstuck: manage my productivity setup from this repo.

mod adopt;
mod apply;
mod ctx;
mod files;
mod manifest;
mod packages;
mod status;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(version, about = "Manage my productivity setup: packages, tools and config files")]
struct Cli {
    /// The dotfiles clone [default: the one rstuck was built from]
    #[arg(long, global = true, env = "RSTUCK_REPO", value_name = "DIR")]
    repo: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install packages and tools, link config files and set up zsh
    Apply {
        /// Only set up config files and the git hook
        #[arg(long, conflicts_with = "no_root")]
        link_only: bool,
        /// Linux: skip system packages and the login shell change
        #[arg(long)]
        no_root: bool,
        /// Show what would change without changing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Show how this machine differs from the repo (exit code 1 if it does)
    Status,
    /// Start tracking config files from your home directory
    Add {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Run the checks and show what would change
        #[arg(long)]
        dry_run: bool,
    },
    /// Manage the package lists
    #[command(subcommand)]
    Pkg(Pkg),
}

#[derive(Subcommand)]
enum Pkg {
    /// Print every package list
    List,
    /// Add packages to a list (then run `rstuck apply` to install them)
    Add {
        #[arg(required = true)]
        names: Vec<String>,
        /// common: same name on apt and dnf. brew: the Brewfile (macOS)
        #[arg(long = "to", value_name = "LIST", required = true, value_delimiter = ',')]
        lists: Vec<packages::List>,
        /// With --to brew: a cask, not a formula
        #[arg(long)]
        cask: bool,
    },
}

fn find_repo(flag: Option<PathBuf>) -> Result<PathBuf> {
    let repo = flag.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let repo = std::path::absolute(repo)?;
    if !repo.join(manifest::FILE).is_file() || !repo.join("home").is_dir() {
        bail!(
            "{} is not a dotfiles repo. Pass --repo or set RSTUCK_REPO.",
            repo.display()
        );
    }
    Ok(repo)
}

fn run(cli: Cli) -> Result<ExitCode> {
    let repo = find_repo(cli.repo)?;
    match cli.command {
        Cmd::Apply { link_only, no_root, dry_run } => {
            let ctx = ctx::Ctx::new(repo, dry_run)?;
            apply::run(&ctx, &apply::Options { link_only, no_root })?;
        }
        Cmd::Status => {
            let count = status::run(&ctx::Ctx::new(repo, true)?)?;
            if count > 0 {
                println!("\n{count} difference(s). `rstuck apply` fixes them.");
                return Ok(ExitCode::FAILURE);
            }
            println!("Up to date.");
        }
        Cmd::Add { files, dry_run } => adopt::run(&ctx::Ctx::new(repo, dry_run)?, &files)?,
        Cmd::Pkg(Pkg::List) => {
            use packages::List;
            for list in [List::Common, List::Apt, List::Dnf] {
                println!("{}: {}", list.file(), packages::read_list(&repo, list)?.join(" "));
            }
            let brew: Vec<_> = packages::read_brewfile(&repo)?.into_iter().map(|p| p.1).collect();
            println!("{}: {}", List::Brew.file(), brew.join(" "));
        }
        Cmd::Pkg(Pkg::Add { names, lists, cask }) => {
            for list in lists {
                for name in &names {
                    let verb = if packages::add(&repo, list, name, cask)? {
                        "added to"
                    } else {
                        "already in"
                    };
                    println!("{name}: {verb} {}", list.file());
                }
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    run(Cli::parse()).unwrap_or_else(|error| {
        eprintln!("error: {error:#}");
        ExitCode::from(2)
    })
}
