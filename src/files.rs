//! Config files: links from home/, ~/.gitconfig, seed and copied files, and the git hook.

use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ctx::{make_parent, output, Ctx};
use crate::manifest::{applies, Manifest};

const MARKER: &str = "# Managed by rstuck. Untracked: git config --global writes here.";
/// Written by the install.sh that rstuck replaced.
const OLD_MARKER: &str = "# Managed by dotfiles install.sh. Untracked: git config --global writes here.";

pub fn shared_gitconfig(ctx: &Ctx) -> PathBuf {
    ctx.repo.join("home/.gitconfig")
}

/// (file in the repo, where it is linked) for everything under home/ except .gitconfig.
pub fn links(ctx: &Ctx) -> Result<Vec<(PathBuf, PathBuf)>> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                walk(&entry.path(), found)?;
            } else if kind.is_file() && entry.file_name() != ".DS_Store" {
                found.push(entry.path());
            }
        }
        Ok(())
    }
    let root = ctx.repo.join("home");
    let mut sources = Vec::new();
    walk(&root, &mut sources)?;
    sources.sort();
    let shared = shared_gitconfig(ctx);
    sources
        .into_iter()
        .filter(|s| *s != shared)
        .map(|s| {
            let dst = ctx.home.join(s.strip_prefix(&root)?);
            Ok((s, dst))
        })
        .collect()
}

pub fn is_linked(source: &Path, destination: &Path) -> bool {
    if fs::read_link(destination).is_ok_and(|target| target == source) {
        return true;
    }
    // A parent directory that already leads into the repo: moving the destination aside
    // would move the repo's own file.
    let is_link = fs::symlink_metadata(destination).is_ok_and(|m| m.is_symlink());
    !is_link
        && matches!(
            (fs::canonicalize(source), fs::canonicalize(destination)),
            (Ok(a), Ok(b)) if a == b
        )
}

pub fn link_all(ctx: &Ctx) -> Result<()> {
    for (source, destination) in links(ctx)? {
        if !is_linked(&source, &destination) {
            ctx.back_up(&destination)?;
            ctx.symlink(&source, &destination)?;
        }
    }
    Ok(())
}

/// Whether ~/.gitconfig is a real file that includes the tracked one.
pub fn gitconfig_ok(ctx: &Ctx) -> bool {
    let path = ctx.home.join(".gitconfig");
    let shared = shared_gitconfig(ctx);
    fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
        && output(Command::new("sh").args([
            "-c",
            "git config -f \"$1\" --get-all include.path || true",
            "sh",
            &path.to_string_lossy(),
        ]))
        .is_ok_and(|out| out.lines().any(|l| Path::new(l) == shared))
}

/// ~/.gitconfig is a real file that includes the tracked one, never a link to it. Tools
/// write with `git config --global`, and actions/checkout copies a link as a link, so even
/// its temporary writes would land in this public repo.
pub fn setup_gitconfig(ctx: &Ctx) -> Result<()> {
    if gitconfig_ok(ctx) {
        return Ok(());
    }
    let path = ctx.home.join(".gitconfig");
    let local = ctx.home.join(".gitconfig.local");
    // Keep an existing git identity: the old ~/.gitconfig becomes the local override.
    // Never copy a file we wrote, which could make the local file include itself.
    if fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
        && fs::symlink_metadata(&local).is_err()
    {
        let old = fs::read(&path)?;
        let ours = String::from_utf8_lossy(&old)
            .lines()
            .any(|l| l == MARKER || l == OLD_MARKER);
        if !ours {
            ctx.write(&local, &old, fs::metadata(&path)?.permissions().mode() & 0o777)?;
            if !ctx.dry_run {
                println!("copied old ~/.gitconfig to ~/.gitconfig.local");
            }
        }
    }
    ctx.back_up(&path)?;
    ctx.write(&path, format!("{MARKER}\n").as_bytes(), 0o644)?;
    ctx.run(
        Command::new("git")
            .arg("config")
            .arg("-f")
            .arg(&path)
            .arg("include.path")
            .arg(shared_gitconfig(ctx)),
    )
}

pub fn copy_differs(source: &Path, destination: &Path) -> Result<bool> {
    let wanted =
        fs::read(source).with_context(|| format!("cannot read {}", source.display()))?;
    Ok(!fs::read(destination).is_ok_and(|have| have == wanted))
}

/// Seed files are created once; copied files are refreshed when they differ.
pub fn apply_rules(ctx: &Ctx, manifest: &Manifest) -> Result<()> {
    for rule in manifest.seeds.iter().filter(|r| applies(&r.os, ctx.os)) {
        let destination = ctx.expand(&rule.to);
        if fs::symlink_metadata(&destination).is_err() {
            let source = ctx.repo.join(&rule.from);
            let contents =
                fs::read(&source).with_context(|| format!("cannot read {}", source.display()))?;
            ctx.write(&destination, &contents, 0o600)?;
        }
    }
    for rule in manifest.copies.iter().filter(|r| applies(&r.os, ctx.os)) {
        let source = ctx.repo.join(&rule.from);
        let destination = ctx.expand(&rule.to);
        if copy_differs(&source, &destination)? {
            ctx.back_up(&destination)?;
            ctx.write(&destination, &fs::read(&source)?, 0o644)?;
        }
    }
    Ok(())
}

/// Where the secret-check hook gets installed, if the repo is a git clone.
pub fn hook_path(ctx: &Ctx) -> Result<Option<PathBuf>> {
    if !ctx.repo.join(".git").exists() {
        return Ok(None);
    }
    let common = output(
        Command::new("git")
            .arg("-C")
            .arg(&ctx.repo)
            .args(["rev-parse", "--git-common-dir"]),
    )?;
    Ok(Some(ctx.repo.join(common.trim()).join("hooks/pre-commit")))
}

/// Block commits that look like they contain secrets. The hook is a copy, so that checking
/// out another branch cannot change what runs on your machine.
pub fn install_hook(ctx: &Ctx) -> Result<()> {
    let Some(hook) = hook_path(ctx)? else {
        return Ok(());
    };
    let git = |args: &[&str]| {
        let mut command = Command::new("git");
        command.arg("-C").arg(&ctx.repo).arg("config").args(args);
        command
    };
    let configured = |args: &[&str]| {
        git(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    if configured(&["--local", "core.hooksPath"]).as_deref() == Some(".githooks") {
        ctx.run(&mut git(&["--local", "--unset", "core.hooksPath"]))?;
    }
    let source = ctx.repo.join(".githooks/pre-commit");
    if ctx.dry_run {
        println!("would install the secret check to {}", hook.display());
    } else {
        make_parent(&hook)?;
        // Remove first so a symlink at the destination is replaced, not written through.
        let _ = fs::remove_file(&hook);
        fs::copy(&source, &hook)
            .with_context(|| format!("cannot install {}", hook.display()))?;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
        if configured(&["core.hooksPath"]).is_some() {
            eprintln!("warning: core.hooksPath is set, so the secret check will not run");
        }
    }
    Ok(())
}
