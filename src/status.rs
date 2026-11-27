//! `rstuck status`: what differs between this machine and the repo. Changes nothing.

use anyhow::Result;
use std::fs;

use crate::ctx::{prepend_path, Ctx};
use crate::manifest::{applies, Manifest};
use crate::{apply, files, packages};

/// Print each difference and return how many there are.
pub fn run(ctx: &Ctx) -> Result<usize> {
    let manifest = Manifest::load(&ctx.repo)?;
    let local = ctx.home.join(".local/bin");
    prepend_path(&[local, ctx.home.join(".cargo/bin")])?;
    let mut problems = Vec::new();

    for name in packages::missing(ctx)? {
        problems.push(format!("package not installed: {name}"));
    }
    for tool in manifest.tools.iter().filter(|t| applies(&t.os, ctx.os)) {
        if !apply::tool_installed(ctx, tool) {
            problems.push(format!("tool not installed: {}", tool.name));
        }
    }
    if !files::gitconfig_ok(ctx) {
        problems.push("~/.gitconfig does not include the tracked git config".into());
    }
    for (source, destination) in files::links(ctx)? {
        if files::is_linked(&source, &destination) {
            continue;
        }
        let state = if fs::symlink_metadata(&destination).is_ok() {
            "not linked to the repo"
        } else {
            "missing"
        };
        problems.push(format!("config {state}: {}", destination.display()));
    }
    for rule in manifest.seeds.iter().filter(|r| applies(&r.os, ctx.os)) {
        if !ctx.expand(&rule.to).exists() {
            problems.push(format!("file missing: {}", rule.to));
        }
    }
    for rule in manifest.copies.iter().filter(|r| applies(&r.os, ctx.os)) {
        if files::copy_differs(&ctx.repo.join(&rule.from), &ctx.expand(&rule.to))? {
            problems.push(format!("copy out of date: {}", rule.to));
        }
    }
    if let Some(hook) = files::hook_path(ctx)? {
        if files::copy_differs(&ctx.repo.join(".githooks/pre-commit"), &hook)? {
            problems.push("secret-check git hook is missing or out of date".into());
        }
    }

    for problem in &problems {
        println!("{problem}");
    }
    Ok(problems.len())
}
