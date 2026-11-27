//! Where things are, and how commands and file changes run (or are only printed).

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Linux,
    Macos,
}

pub struct Ctx {
    pub repo: PathBuf,
    pub home: PathBuf,
    pub os: Os,
    pub dry_run: bool,
    backup: PathBuf,
}

impl Ctx {
    pub fn new(repo: PathBuf, dry_run: bool) -> Result<Self> {
        let os = match std::env::consts::OS {
            "linux" => Os::Linux,
            "macos" => Os::Macos,
            other => bail!("unsupported OS: {other}"),
        };
        let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
        if !home.is_absolute() {
            bail!("HOME must be an absolute path");
        }
        let stamp = output(Command::new("date").arg("+%Y%m%d-%H%M%S"))?;
        let backup = home
            .join(".dotfiles-backup")
            .join(format!("{}-{}", stamp.trim(), std::process::id()));
        Ok(Self { repo, home, os, dry_run, backup })
    }

    /// Expand a leading `~/`.
    pub fn expand(&self, path: &str) -> PathBuf {
        match path.strip_prefix("~/") {
            Some(rest) => self.home.join(rest),
            None => PathBuf::from(path),
        }
    }

    /// Run a command with the terminal attached. Only printed on a dry run.
    pub fn run(&self, command: &mut Command) -> Result<()> {
        if self.dry_run {
            println!("would run: {}", describe(command));
            return Ok(());
        }
        let status = command
            .status()
            .with_context(|| format!("cannot run: {}", describe(command)))?;
        if !status.success() {
            bail!("failed ({status}): {}", describe(command));
        }
        Ok(())
    }

    /// A command that runs as root: directly if we are root, otherwise through sudo.
    pub fn root_command(&self, argv: &[String]) -> Result<Command> {
        let is_root = output(Command::new("id").arg("-u"))?.trim() == "0";
        let mut command = if is_root {
            Command::new(&argv[0])
        } else if which("sudo").is_some() {
            let mut sudo = Command::new("sudo");
            sudo.arg(&argv[0]);
            sudo
        } else {
            bail!(
                "need root or sudo for: {}  (or re-run with --no-root)",
                argv.join(" ")
            );
        };
        command.args(&argv[1..]);
        Ok(command)
    }

    /// Move an existing file out of the way, keeping its path relative to home.
    pub fn back_up(&self, path: &Path) -> Result<()> {
        if fs::symlink_metadata(path).is_err() {
            return Ok(());
        }
        let relative = path
            .strip_prefix(&self.home)
            .with_context(|| format!("not inside HOME: {}", path.display()))?;
        let to = self.backup.join(relative);
        if self.dry_run {
            println!("would back up {}", path.display());
            return Ok(());
        }
        make_parent(&to)?;
        // `mv` also works when home spans filesystems.
        self.run(Command::new("mv").arg(path).arg(&to))?;
        println!("backed up {} -> {}", path.display(), to.display());
        Ok(())
    }

    pub fn symlink(&self, source: &Path, link: &Path) -> Result<()> {
        if self.dry_run {
            println!("would link {} -> {}", link.display(), source.display());
            return Ok(());
        }
        make_parent(link)?;
        std::os::unix::fs::symlink(source, link)
            .with_context(|| format!("cannot link {}", link.display()))
    }

    /// Write a new file, or replace one, with the given permissions.
    pub fn write(&self, path: &Path, contents: &[u8], mode: u32) -> Result<()> {
        if self.dry_run {
            println!("would write {}", path.display());
            return Ok(());
        }
        make_parent(path)?;
        let mut file = fs::File::create(path)
            .with_context(|| format!("cannot write {}", path.display()))?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(contents)?;
        Ok(())
    }
}

pub fn make_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    Ok(())
}

pub fn describe(command: &Command) -> String {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(OsStr::to_string_lossy)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run a read-only command and return its standard output. Runs even on a dry run.
pub fn output(command: &mut Command) -> Result<String> {
    let out = command
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("cannot run: {}", describe(command)))?;
    if !out.status.success() {
        bail!("failed ({}): {}", out.status, describe(command));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether a command succeeds, with all of its output hidden.
pub fn succeeds(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// First executable with this name on PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.mode() & 0o111 != 0))
}

/// Put these directories first on PATH, for this process and the commands it starts.
pub fn prepend_path(dirs: &[PathBuf]) -> Result<()> {
    let old = std::env::var_os("PATH").unwrap_or_default();
    let all = dirs.iter().cloned().chain(std::env::split_paths(&old));
    std::env::set_var("PATH", std::env::join_paths(all)?);
    Ok(())
}
