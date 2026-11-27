//! rstuck.toml: the tools, seed files and copied files.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use crate::ctx::Os;

pub const FILE: &str = "rstuck.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default, rename = "tool")]
    pub tools: Vec<Tool>,
    #[serde(default, rename = "seed")]
    pub seeds: Vec<FileRule>,
    #[serde(default, rename = "copy")]
    pub copies: Vec<FileRule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub os: Vec<Os>,
    pub command: Option<String>,
    pub creates: Option<String>,
    pub run: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRule {
    #[serde(default)]
    pub os: Vec<Os>,
    /// Path inside the repo.
    pub from: String,
    /// Destination, starting with `~/`.
    pub to: String,
}

/// An empty `os` list means every OS.
pub fn applies(list: &[Os], os: Os) -> bool {
    list.is_empty() || list.contains(&os)
}

impl Manifest {
    pub fn load(repo: &Path) -> Result<Self> {
        let path = repo.join(FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Manifest = toml::from_str(text)?;
        let mut names = HashSet::new();
        for tool in &manifest.tools {
            if !names.insert(&tool.name) {
                bail!("tool {:?} is listed twice", tool.name);
            }
            if tool.command.is_some() == tool.creates.is_some() {
                bail!("tool {:?} needs exactly one of `command` or `creates`", tool.name);
            }
            if let Some(path) = &tool.creates {
                home_relative(path)?;
            }
        }
        for rule in manifest.seeds.iter().chain(&manifest.copies) {
            if !is_plain_relative(Path::new(&rule.from)) {
                bail!("`from` must be a path inside the repo: {}", rule.from);
            }
            home_relative(&rule.to)?;
        }
        Ok(manifest)
    }
}

fn is_plain_relative(path: &Path) -> bool {
    path.components().next().is_some()
        && path.components().all(|c| matches!(c, Component::Normal(_)))
}

/// The part after `~/`. Rejects anything that could leave the home directory.
pub fn home_relative(path: &str) -> Result<PathBuf> {
    match path.strip_prefix("~/") {
        Some(rest) if is_plain_relative(Path::new(rest)) => Ok(PathBuf::from(rest)),
        _ => bail!("path must start with ~/ and stay inside it: {path}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_manifest_is_valid() {
        let manifest = Manifest::parse(include_str!("../rstuck.toml")).unwrap();
        let names: Vec<_> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["antidote", "fzf", "starship", "zoxide"]);
    }

    #[test]
    fn rejects_mistakes() {
        for bad in [
            "[[tool]]\nname='a'\nrun='x'",
            "[[tool]]\nname='a'\nrun='x'\ncommand='a'\ncreates='~/a'",
            "[[tool]]\nname='a'\nrun='x'\ncommand='a'\n[[tool]]\nname='a'\nrun='x'\ncommand='a'",
            "[[tool]]\nname='a'\nrun='x'\ncommand='a'\nos=['windows']",
            "[[tool]]\nname='a'\nrun='x'\ncommand='a'\ntypo=1",
            "[[seed]]\nfrom='../x'\nto='~/x'",
            "[[seed]]\nfrom='x'\nto='/etc/x'",
            "[[copy]]\nfrom='x'\nto='~/../x'",
            "[[tools]]\nname='a'",
        ] {
            assert!(Manifest::parse(bad).is_err(), "accepted: {bad}");
        }
    }
}
