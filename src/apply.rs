//! `rstuck apply`: bring this machine in line with the repo.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::ctx::{output, prepend_path, succeeds, which, Ctx, Os};
use crate::manifest::{applies, Manifest, Tool};
use crate::{files, packages};

/// zsh-fast-alias-tips uses Rust edition 2024.
const MIN_CARGO: (u32, u32) = (1, 85);

pub struct Options {
    pub link_only: bool,
    pub no_root: bool,
}

pub fn run(ctx: &Ctx, options: &Options) -> Result<()> {
    if options.no_root && ctx.os == Os::Macos {
        bail!("--no-root is for Linux");
    }
    // Fail on a broken manifest before changing anything.
    let manifest = Manifest::load(&ctx.repo)?;

    if !options.link_only {
        let local = ctx.home.join(".local");
        prepend_path(&[local.join("bin"), ctx.home.join(".cargo/bin")])?;
        match ctx.os {
            Os::Macos => brew(ctx)?,
            Os::Linux => {
                if !options.no_root {
                    packages::install_linux(ctx)?;
                }
                require(ctx, &["zsh", "git", "curl", "tar", "gcc"])?;
            }
        }
        if !ctx.dry_run {
            for dir in ["bin", "share"] {
                std::fs::create_dir_all(local.join(dir))?;
            }
        }
        for tool in manifest.tools.iter().filter(|t| applies(&t.os, ctx.os)) {
            install_tool(ctx, tool)?;
        }
    }

    files::setup_gitconfig(ctx)?;
    files::link_all(ctx)?;
    files::apply_rules(ctx, &manifest)?;
    files::install_hook(ctx)?;

    if !options.link_only {
        zsh_plugins(ctx)?;
        // Linux servers usually start users in bash.
        if ctx.os == Os::Linux && !options.no_root {
            login_shell(ctx)?;
        }
    }
    next_steps(ctx);
    Ok(())
}

fn require(ctx: &Ctx, commands: &[&str]) -> Result<()> {
    let missing: Vec<_> = commands.iter().filter(|c| which(c).is_none()).copied().collect();
    // On a dry run the package step has not installed anything yet.
    if !missing.is_empty() && !ctx.dry_run {
        bail!("missing: {}. Install them or run without --no-root.", missing.join(" "));
    }
    Ok(())
}

fn brew(ctx: &Ctx) -> Result<()> {
    prepend_path(&[PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")])?;
    if which("brew").is_none() {
        bail!("Install Homebrew first: https://brew.sh");
    }
    let mut file = std::ffi::OsString::from("--file=");
    file.push(ctx.repo.join("Brewfile"));
    ctx.run(Command::new("brew").arg("bundle").arg(file))?;
    if !std::path::Path::new("/Applications/iTerm.app").is_dir() {
        ctx.run(Command::new("brew").args(["install", "--cask", "iterm2"]))?;
    }
    Ok(())
}

pub fn tool_installed(ctx: &Ctx, tool: &Tool) -> bool {
    match (&tool.command, &tool.creates) {
        (Some(command), _) => which(command).is_some(),
        (None, Some(path)) => ctx.expand(path).exists(),
        (None, None) => false,
    }
}

fn install_tool(ctx: &Ctx, tool: &Tool) -> Result<()> {
    if tool_installed(ctx, tool) {
        return Ok(());
    }
    if ctx.dry_run {
        println!("would install tool: {}", tool.name);
        return Ok(());
    }
    println!("installing {}", tool.name);
    ctx.run(
        Command::new("bash")
            .args(["-euo", "pipefail", "-c", &tool.run])
            .current_dir(&ctx.home),
    )
    .map_err(|_| anyhow::anyhow!("could not install tool: {}", tool.name))
}

/// Homebrew's antidote on macOS, a per-user clone on Linux.
fn antidote_zsh(ctx: &Ctx) -> PathBuf {
    if which("brew").is_some() {
        if let Ok(prefix) = output(Command::new("brew").arg("--prefix")) {
            let path = PathBuf::from(prefix.trim()).join("opt/antidote/share/antidote/antidote.zsh");
            if path.is_file() {
                return path;
            }
        }
    }
    ctx.home.join(".local/share/antidote/antidote.zsh")
}

fn zsh_plugins(ctx: &Ctx) -> Result<()> {
    if ctx.dry_run {
        println!("would set up zsh plugins and build the alias-tips helper if needed");
        return Ok(());
    }
    // The first interactive shell makes antidote clone the plugins.
    ctx.run(Command::new("zsh").args(["-ilc", "true"]))?;
    // zsh-fast-alias-tips needs a compiled helper.
    let tips = output(
        Command::new("zsh")
            .args(["-c", "source \"$1\"; antidote path sei40kr/zsh-fast-alias-tips", "zsh"])
            .arg(antidote_zsh(ctx)),
    )?;
    let tips = PathBuf::from(tips.trim());
    if !tips.join("target/release/alias-matcher").is_file() {
        ensure_cargo(ctx)?;
        ctx.run(
            Command::new("cargo")
                .args(["build", "--release", "--manifest-path"])
                .arg(tips.join("Cargo.toml"))
                // A shared target directory would put the helper where the plugin cannot find it.
                .env_remove("CARGO_TARGET_DIR"),
        )?;
    }
    Ok(())
}

/// `cargo 1.85.0 (...)` -> (1, 85)
fn cargo_version(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.split_whitespace().nth(1)?.split(['.', '-']);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

fn ensure_cargo(ctx: &Ctx) -> Result<()> {
    if which("cargo").is_some()
        && output(Command::new("cargo").arg("--version"))
            .ok()
            .and_then(|v| cargo_version(&v))
            .is_some_and(|v| v >= MIN_CARGO)
    {
        return Ok(());
    }
    match ctx.os {
        Os::Macos => ctx.run(Command::new("brew").args(["install", "rust"])),
        Os::Linux => ctx.run(Command::new("bash").args([
            "-euo",
            "pipefail",
            "-c",
            "curl -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --no-modify-path",
        ])),
    }
}

fn login_shell(ctx: &Ctx) -> Result<()> {
    let Some(zsh) = which("zsh") else {
        if ctx.dry_run {
            println!("would change the login shell to zsh");
            return Ok(());
        }
        bail!("zsh is not installed");
    };
    let me = output(Command::new("id").arg("-un"))?.trim().to_string();
    let entry = output(Command::new("getent").args(["passwd", &me]))
        .context("cannot look up the login shell")?;
    if entry.trim().split(':').nth(6).map(std::path::Path::new) == Some(zsh.as_path()) {
        return Ok(());
    }
    // /bin/zsh and /usr/bin/zsh are often the same file.
    let current = entry.trim().split(':').nth(6).unwrap_or_default().to_string();
    if current.ends_with("/zsh") && succeeds(Command::new("test").args([&current, "-ef"]).arg(&zsh)) {
        return Ok(());
    }
    let zsh = zsh.to_string_lossy().into_owned();
    let argv = ["usermod".to_string(), "-s".into(), zsh.clone(), me];
    if ctx.root_command(&argv).and_then(|mut c| ctx.run(&mut c)).is_err() {
        eprintln!("warning: could not change login shell; run: chsh -s {zsh}");
    }
    Ok(())
}

fn next_steps(ctx: &Ctx) {
    if ctx.dry_run {
        println!("\nDry run: nothing was changed.");
        return;
    }
    let terminal = match ctx.os {
        Os::Macos => {
            "3. iTerm2 > Settings > Profiles > \"Source Mac\" > Other Actions > Set as Default.\n  \
             4. Open a new terminal window."
        }
        Os::Linux => {
            "3. Log out and back in (or run: exec zsh -l). The prompt uses no special\n     \
             font; the font is set on the machine you connect from."
        }
    };
    println!(
        "\nDone. Next:\n  \
         1. Put secrets and machine-specific settings in ~/.zshrc.local\n     \
         (if an old .zshrc was backed up, copy what you still need from it).\n  \
         2. Set your git identity in ~/.gitconfig.local (see README).\n  {terminal}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_cargo_versions_as_numbers() {
        assert_eq!(cargo_version("cargo 1.100.0 (abc 2026-11-05)"), Some((1, 100)));
        assert_eq!(cargo_version("cargo 1.85.0-nightly (abc)"), Some((1, 85)));
        assert!(cargo_version("cargo 1.100.0").unwrap() >= MIN_CARGO);
        assert!(cargo_version("cargo 1.9.0").unwrap() < MIN_CARGO);
        assert_eq!(cargo_version(""), None);
    }
}
