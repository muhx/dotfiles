//! Package lists (packages/*.txt, Brewfile) and the Linux system install plan.

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::ctx::{output, succeeds, which, Ctx};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum List {
    Common,
    Apt,
    Dnf,
    Brew,
}

impl List {
    pub fn file(self) -> &'static str {
        match self {
            List::Common => "packages/common.txt",
            List::Apt => "packages/apt.txt",
            List::Dnf => "packages/dnf.txt",
            List::Brew => "Brewfile",
        }
    }
}

/// Names reach a package manager running as root, so only plain names are allowed. That
/// excludes options, and a trailing "-", which makes apt-get install remove the package.
pub fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && (last.is_ascii_alphanumeric() || *last == b'+')
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || b"+._-".contains(b))
}

/// Homebrew names may also be taps or versions: `user/tap/name`, `python@3.12`.
fn valid_brew_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "+._-@/".contains(c))
}

/// One package name per line. `#` starts a comment.
pub fn read_list(repo: &Path, list: List) -> Result<Vec<String>> {
    let path = repo.join(list.file());
    let text = fs::read_to_string(&path)
        .with_context(|| format!("missing package list: {}", path.display()))?;
    let mut names = Vec::new();
    for line in text.lines() {
        let name = line.split('#').next().unwrap_or_default().trim();
        if name.is_empty() {
            continue;
        }
        if !valid_name(name) {
            bail!("bad package name in {}: {name}", path.display());
        }
        names.push(name.to_string());
    }
    Ok(names)
}

/// `brew "x"` and `cask "x"` lines.
pub fn read_brewfile(repo: &Path) -> Result<Vec<(String, String)>> {
    let path = repo.join(List::Brew.file());
    let text =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let (kind, rest) = line.trim().split_once(' ')?;
            let name = rest.trim().strip_prefix('"')?.split('"').next()?;
            matches!(kind, "brew" | "cask").then(|| (kind.to_string(), name.to_string()))
        })
        .collect())
}

/// Add a package to a list. Returns false if it was already there.
pub fn add(repo: &Path, list: List, name: &str, cask: bool) -> Result<bool> {
    if cask && list != List::Brew {
        bail!("--cask only applies to --to brew");
    }
    let (valid, line, present) = if list == List::Brew {
        let kind = if cask { "cask" } else { "brew" };
        let present = read_brewfile(repo)?.iter().any(|(k, n)| k == kind && n == name);
        (valid_brew_name(name), format!("{kind} \"{name}\""), present)
    } else {
        let present = read_list(repo, list)?.iter().any(|n| n == name);
        (valid_name(name), name.to_string(), present)
    };
    if !valid {
        bail!("not a valid package name: {name:?}");
    }
    // rstuck installs these itself.
    if name == "gh" && list != List::Brew {
        bail!("gh is installed from GitHub's own repo; keep it out of the Linux lists");
    }
    if name == "curl" && matches!(list, List::Common | List::Dnf) {
        bail!("curl is added on dnf distros only when missing (curl-minimal conflicts with it)");
    }
    if present {
        return Ok(false);
    }
    let path = repo.join(list.file());
    let mut text = fs::read_to_string(&path)?;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&line);
    text.push('\n');
    fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Apt,
    Dnf { epel: bool },
}

/// Pick the package manager from the contents of /etc/os-release.
pub fn family(os_release: &str) -> Result<Family> {
    let field = |key: &str| {
        os_release
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
            .unwrap_or_default()
    };
    let (id, like) = (field("ID"), field("ID_LIKE"));
    let all: Vec<&str> = id.split_whitespace().chain(like.split_whitespace()).collect();
    let is = |name: &str| all.contains(&name);
    if is("debian") || is("ubuntu") {
        Ok(Family::Apt)
    } else if is("rhel") || is("fedora") || is("centos") {
        // htop lives in EPEL. Fedora has no epel-release and does not need it.
        Ok(Family::Dnf { epel: !is("fedora") || is("rhel") })
    } else {
        bail!("unsupported Linux distribution: {id} (need apt or dnf)")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// A command run as root.
    Root(Vec<String>),
    /// Download a URL, then write it to a root-owned file.
    Fetch { url: &'static str, to: &'static str },
}

fn root(words: &[&str]) -> Step {
    Step::Root(words.iter().map(|w| w.to_string()).collect())
}

pub struct Host {
    pub has_gh: bool,
    pub has_curl: bool,
    /// `dpkg --print-architecture`; empty on dnf distros.
    pub arch: String,
}

/// Every root step needed for system packages. All lists are read and validated first,
/// so a bad list stops the install before any root command runs.
pub fn plan(repo: &Path, family: Family, host: &Host) -> Result<Vec<Step>> {
    let mut names = read_list(repo, List::Common)?;
    let mut steps = Vec::new();
    match family {
        Family::Apt => {
            names.extend(read_list(repo, List::Apt)?);
            let apt = ["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "-yq"];
            let install = |names: &[String]| {
                let mut argv: Vec<String> = apt.iter().map(|s| s.to_string()).collect();
                argv.push("install".into());
                argv.extend_from_slice(names);
                Step::Root(argv)
            };
            let update = || root(&[apt[0], apt[1], apt[2], apt[3], "update"]);
            steps.push(update());
            steps.push(install(&names));
            if !host.has_gh {
                const KEY: &str = "/etc/apt/keyrings/githubcli-archive-keyring.gpg";
                const SOURCE: &str = "/etc/apt/sources.list.d/github-cli.list";
                steps.push(root(&["mkdir", "-p", "-m", "755", "/etc/apt/keyrings"]));
                steps.push(Step::Fetch {
                    url: "https://cli.github.com/packages/githubcli-archive-keyring.gpg",
                    to: KEY,
                });
                steps.push(root(&["chmod", "go+r", KEY]));
                // The arch is the only outside value in this root shell: pass it as an argument.
                let line = format!(
                    "deb [arch={} signed-by={KEY}] https://cli.github.com/packages stable main",
                    host.arch
                );
                steps.push(root(&["sh", "-c", "printf '%s\\n' \"$1\" > \"$2\"", "sh", &line, SOURCE]));
                steps.push(update());
                steps.push(install(&["gh".to_string()]));
            }
        }
        Family::Dnf { epel } => {
            names.extend(read_list(repo, List::Dnf)?);
            if epel {
                steps.push(root(&["dnf", "install", "-y", "epel-release"]));
            }
            // Rocky ships curl-minimal, which conflicts with curl: only add curl if absent.
            if !host.has_curl {
                names.push("curl".into());
            }
            let mut argv: Vec<String> = ["dnf", "install", "-y"].map(String::from).to_vec();
            argv.extend(names);
            steps.push(Step::Root(argv));
            if !host.has_gh {
                steps.push(Step::Fetch {
                    url: "https://cli.github.com/packages/rpm/gh-cli.repo",
                    to: "/etc/yum.repos.d/gh-cli.repo",
                });
                steps.push(root(&["dnf", "install", "-y", "gh"]));
            }
        }
    }
    Ok(steps)
}

fn host_family() -> Result<Family> {
    family(&fs::read_to_string("/etc/os-release").context("cannot read /etc/os-release")?)
}

pub fn install_linux(ctx: &Ctx) -> Result<()> {
    let family = host_family()?;
    let arch = match family {
        Family::Apt => output(Command::new("dpkg").arg("--print-architecture"))?.trim().to_string(),
        Family::Dnf { .. } => String::new(),
    };
    let host = Host { has_gh: which("gh").is_some(), has_curl: which("curl").is_some(), arch };
    for step in plan(&ctx.repo, family, &host)? {
        match step {
            Step::Root(argv) => ctx.run(&mut ctx.root_command(&argv)?)?,
            Step::Fetch { url, to } => fetch_as_root(ctx, url, to)?,
        }
    }
    Ok(())
}

fn fetch_as_root(ctx: &Ctx, url: &str, to: &str) -> Result<()> {
    if ctx.dry_run {
        println!("would download {url} to {to}");
        return Ok(());
    }
    // Download completely before writing, so a failed download leaves no partial file.
    let temp = tempfile::NamedTempFile::new()?;
    ctx.run(Command::new("curl").args(["-fsSL", "-o"]).arg(temp.path()).arg(url))?;
    let argv = [
        "install".to_string(),
        "-m".into(),
        "644".into(),
        temp.path().to_string_lossy().into_owned(),
        to.into(),
    ];
    ctx.run(&mut ctx.root_command(&argv)?)
}

/// Listed system packages that are not installed.
pub fn missing(ctx: &Ctx) -> Result<Vec<String>> {
    if ctx.os == crate::ctx::Os::Macos {
        if which("brew").is_none() {
            bail!("Homebrew is not installed");
        }
        return Ok(read_brewfile(&ctx.repo)?
            .into_iter()
            .filter(|(kind, name)| {
                let flag = if kind == "cask" { "--cask" } else { "--formula" };
                !succeeds(Command::new("brew").args(["list", flag, name]))
            })
            .map(|(_, name)| name)
            .collect());
    }
    let family = host_family()?;
    let mut names = read_list(&ctx.repo, List::Common)?;
    names.extend(read_list(
        &ctx.repo,
        if family == Family::Apt { List::Apt } else { List::Dnf },
    )?);
    Ok(names
        .into_iter()
        .filter(|name| match family {
            Family::Apt => !output(Command::new("sh").args([
                "-c",
                "dpkg-query -W -f='${db:Status-Status}' \"$1\" 2>/dev/null || true",
                "sh",
                name,
            ]))
            .is_ok_and(|s| s == "installed"),
            Family::Dnf { .. } => !succeeds(Command::new("rpm").args(["-q", "--whatprovides", name])),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_with(common: &str, apt: &str, dnf: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        fs::create_dir(path.join("packages")).unwrap();
        fs::write(path.join("packages/common.txt"), common).unwrap();
        fs::write(path.join("packages/apt.txt"), apt).unwrap();
        fs::write(path.join("packages/dnf.txt"), dnf).unwrap();
        fs::write(path.join("Brewfile"), "brew \"fzf\"\ncask \"font-fira-code\"").unwrap();
        (dir, path)
    }

    /// Package names passed to every `install` command in the plan.
    fn installed(steps: &[Step]) -> Vec<String> {
        steps
            .iter()
            .filter_map(|s| match s {
                Step::Root(argv) if !argv.contains(&"epel-release".to_string()) => {
                    let at = argv.iter().position(|a| a == "install")?;
                    Some(argv[at + 1..].iter().filter(|a| *a != "-y").cloned().collect::<Vec<_>>())
                }
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn distros_pick_the_right_family() {
        assert_eq!(family("ID=ubuntu\nID_LIKE=debian").unwrap(), Family::Apt);
        assert_eq!(family("ID=debian").unwrap(), Family::Apt);
        assert_eq!(
            family("ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"").unwrap(),
            Family::Dnf { epel: true }
        );
        assert_eq!(family("ID=fedora").unwrap(), Family::Dnf { epel: false });
        assert!(family("ID=arch\nVERSION_ID=1").is_err());
    }

    #[test]
    fn real_lists_produce_the_expected_installs() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        let present = Host { has_gh: true, has_curl: true, arch: "amd64".into() };
        let apt = installed(&plan(repo, Family::Apt, &present).unwrap());
        let dnf = installed(&plan(repo, Family::Dnf { epel: true }, &present).unwrap());
        for name in ["zsh", "git", "tar", "gzip", "tmux", "htop", "tree", "gcc"] {
            assert!(apt.iter().any(|n| n == name), "apt: {name}");
            assert!(dnf.iter().any(|n| n == name), "dnf: {name}");
        }
        for name in ["curl", "ca-certificates", "libc6-dev"] {
            assert!(apt.iter().any(|n| n == name), "apt: {name}");
            assert!(!dnf.iter().any(|n| n == name), "dnf got {name}");
        }
    }

    #[test]
    fn gh_epel_and_curl_are_added_only_when_needed() {
        let (_dir, repo) = repo_with("zsh\n", "", "");
        let bare = Host { has_gh: false, has_curl: false, arch: "arm64".into() };
        let apt = plan(&repo, Family::Apt, &bare).unwrap();
        assert_eq!(installed(&apt), ["zsh", "gh"]);
        assert!(apt.iter().any(|s| matches!(s, Step::Fetch { .. })));
        let rocky = plan(&repo, Family::Dnf { epel: true }, &bare).unwrap();
        assert_eq!(rocky[0], root(&["dnf", "install", "-y", "epel-release"]));
        assert_eq!(installed(&rocky), ["zsh", "curl", "gh"]);
        let fedora = plan(&repo, Family::Dnf { epel: false }, &bare).unwrap();
        assert!(!format!("{fedora:?}").contains("epel"));
    }

    #[test]
    fn list_syntax_and_bad_names() {
        let (_dir, repo) = repo_with("", "# note\n\n  g++  # trailing\r\nlast-no-newline", "");
        assert_eq!(read_list(&repo, List::Apt).unwrap(), ["g++", "last-no-newline"]);
        for bad in ["--allow-unauthenticated", "zsh-", "two names", "*", "x;id", ".x"] {
            fs::write(repo.join("packages/apt.txt"), bad).unwrap();
            let host = Host { has_gh: true, has_curl: true, arch: "amd64".into() };
            assert!(plan(&repo, Family::Apt, &host).is_err(), "accepted {bad}");
        }
        fs::remove_file(repo.join("packages/apt.txt")).unwrap();
        assert!(read_list(&repo, List::Apt).is_err());
    }

    #[test]
    fn add_appends_once_and_guards_names() {
        let (_dir, repo) = repo_with("zsh", "", "");
        assert!(add(&repo, List::Common, "ripgrep", false).unwrap());
        assert!(!add(&repo, List::Common, "ripgrep", false).unwrap());
        assert_eq!(read_list(&repo, List::Common).unwrap(), ["zsh", "ripgrep"]);
        assert!(add(&repo, List::Brew, "python@3.12", false).unwrap());
        assert!(!add(&repo, List::Brew, "font-fira-code", true).unwrap());
        assert_eq!(read_brewfile(&repo).unwrap().len(), 3);
        for (list, name) in [
            (List::Common, "x-"),
            (List::Common, "a\nb"),
            (List::Common, "gh"),
            (List::Dnf, "curl"),
            (List::Brew, "x\"\nsystem \"id"),
        ] {
            assert!(add(&repo, list, name, false).is_err(), "accepted {name:?}");
        }
        assert!(add(&repo, List::Apt, "vim", true).is_err());
    }
}
