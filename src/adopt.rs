//! `rstuck add`: start tracking a config file. The repo is public, so this is cautious.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ctx::{make_parent, succeeds, Ctx};
use crate::files;

/// Directories whose contents are credentials.
const PRIVATE_DIRS: &[&str] = &[
    ".ssh", ".gnupg", ".aws", ".azure", ".kube", ".docker", ".password-store", ".config/gh",
    ".config/gcloud", ".dotfiles-backup",
];
/// File names that hold credentials, history or per-machine settings.
const PRIVATE_FILES: &[&str] = &[
    ".gitconfig", ".netrc", ".npmrc", ".pypirc", ".git-credentials", ".pgpass", ".my.cnf",
    ".env", "credentials", "credentials.json", ".credentials.json", "id_rsa", "id_ed25519",
];

/// Why a path (relative to home) must never be tracked, if it must not.
fn private_reason(relative: &Path) -> Option<&'static str> {
    // Case-insensitive, as macOS file systems usually are.
    let path = relative.to_string_lossy().to_lowercase();
    let name = path.rsplit('/').next().unwrap_or_default();
    if PRIVATE_DIRS.iter().any(|d| Path::new(&path).starts_with(d)) {
        Some("it is in a directory that holds credentials or backups")
    } else if PRIVATE_FILES.contains(&name) || name.starts_with(".env.") {
        Some("files with this name usually hold credentials or personal settings")
    } else if name.ends_with(".local") {
        Some("*.local files are for this machine only")
    } else if name.ends_with("history") {
        Some("history files can contain secrets")
    } else if name.ends_with(".pem") || name.ends_with(".key") {
        Some("it looks like a private key")
    } else {
        None
    }
}

/// Run the repo's secret check on one file, in a throwaway git repository.
fn scan(ctx: &Ctx, relative: &Path, contents: &[u8]) -> Result<()> {
    // The check skips binary files, so it cannot vouch for them.
    if contents.contains(&0) {
        bail!("binary files cannot be checked for secrets");
    }
    let scratch = tempfile::tempdir()?;
    let staged = scratch.path().join(relative);
    make_parent(&staged)?;
    fs::write(&staged, contents)?;
    let git = |args: &[&str]| {
        let mut command = Command::new("git");
        command.arg("-C").arg(scratch.path()).args(args);
        command
    };
    if !succeeds(&mut git(&["init", "-q"])) || !succeeds(&mut git(&["add", "-f", "-A"])) {
        bail!("could not prepare the secret check");
    }
    let status = Command::new("bash")
        .arg(ctx.repo.join(".githooks/pre-commit"))
        .current_dir(scratch.path())
        .status()
        .context("could not run the secret check")?;
    if !status.success() {
        bail!("the secret check rejected this file (see above)");
    }
    Ok(())
}

struct Candidate {
    original: PathBuf,
    tracked: PathBuf,
    contents: Vec<u8>,
}

/// Check one file. Returns None if it is already tracked and linked.
fn check(ctx: &Ctx, path: &Path) -> Result<Option<Candidate>> {
    let absolute = std::path::absolute(path)?;
    let meta = fs::symlink_metadata(&absolute).context("no such file")?;
    let real = fs::canonicalize(&absolute)?;
    let repo = fs::canonicalize(&ctx.repo)?;
    if real.starts_with(repo.join("home")) {
        return Ok(None);
    }
    if !meta.is_file() {
        bail!("only regular files can be added (not directories or links)");
    }
    if real.starts_with(&repo) {
        bail!("it is already inside the repo");
    }
    // Resolve linked directories, so they cannot hide where a file really is.
    let home = fs::canonicalize(&ctx.home)?;
    let relative = real
        .strip_prefix(&home)
        .context("only files inside your home directory can be added")?;
    if let Some(reason) = private_reason(relative) {
        bail!("refusing to track it: {reason}");
    }
    let tracked = ctx.repo.join("home").join(relative);
    let contents = fs::read(&real)?;
    if fs::symlink_metadata(&tracked).is_ok()
        && !fs::read(&tracked).is_ok_and(|have| have == contents)
    {
        bail!("the repo already has a different {}", tracked.display());
    }
    // A repo that ignores the file would silently never commit it.
    if ctx.repo.join(".git").exists()
        && succeeds(
            Command::new("git")
                .arg("-C")
                .arg(&ctx.repo)
                .args(["check-ignore", "-q"])
                .arg(Path::new("home").join(relative)),
        )
    {
        bail!("the repo's git ignore rules exclude it");
    }
    scan(ctx, relative, &contents)?;
    Ok(Some(Candidate { original: ctx.home.join(relative), tracked, contents }))
}

pub fn run(ctx: &Ctx, paths: &[PathBuf]) -> Result<()> {
    // Check everything first, so one bad file means nothing changes.
    let mut candidates = Vec::new();
    for path in paths {
        match check(ctx, path).with_context(|| format!("cannot add {}", path.display()))? {
            Some(candidate) => candidates.push(candidate),
            None => println!("already tracked: {}", path.display()),
        }
    }
    for Candidate { original, tracked, contents } in candidates {
        if files::is_linked(&tracked, &original) {
            continue;
        }
        if !ctx.dry_run {
            make_parent(&tracked)?;
            fs::write(&tracked, &contents)?;
            fs::set_permissions(&tracked, fs::metadata(&original)?.permissions())?;
        }
        ctx.back_up(&original)?;
        ctx.symlink(&tracked, &original)?;
        if !ctx.dry_run {
            println!("added {}", tracked.display());
        }
    }
    println!("This repo is PUBLIC. Read the file and `git diff` before you commit: the secret check only catches common patterns.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_paths_are_refused() {
        for path in [
            ".ssh/config", ".SSH/config", ".config/gh/hosts.yml", ".zshrc.local", ".gitconfig",
            ".zsh_history", ".aws/credentials", "app/.env", "app/.env.production", "x/server.pem",
        ] {
            assert!(private_reason(Path::new(path)).is_some(), "allowed {path}");
        }
        for path in [".vimrc", ".config/tmux/tmux.conf", ".config/ghostty/config"] {
            assert!(private_reason(Path::new(path)).is_none(), "refused {path}");
        }
    }
}
