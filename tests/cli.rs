//! End-to-end checks against a copy of the repo and a throwaway HOME.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(root.path().join("home")).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        for item in [
            "home", "packages", "iterm2", ".githooks", ".gitignore", "Brewfile", "rstuck.toml",
            "zshrc.local.example",
        ] {
            let status = Command::new("cp")
                .arg("-R")
                .arg(source.join(item))
                .arg(&repo)
                .status()
                .unwrap();
            assert!(status.success());
        }
        Self { root }
    }

    fn repo(&self) -> PathBuf {
        self.root.path().join("repo")
    }

    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    fn command(&self, program: &str, home: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .env("HOME", home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_CONFIG_GLOBAL")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("RSTUCK_REPO")
            .current_dir(home);
        command
    }

    fn rstuck_in(&self, repo: &Path, home: &Path, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_rstuck"), home)
            .arg("--repo")
            .arg(repo)
            .args(args)
            .output()
            .unwrap()
    }

    fn rstuck(&self, args: &[&str]) -> Output {
        self.rstuck_in(&self.repo(), &self.home(), args)
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.rstuck(args);
        assert!(out.status.success(), "{args:?}: {}", text(&out));
        text(&out)
    }

    fn link(&self) {
        self.ok(&["apply", "--link-only"]);
    }

    fn identity(&self) -> String {
        let out = self
            .command("git", &self.home())
            .args(["config", "--global", "--includes", "user.name"])
            .output()
            .unwrap();
        text(&out).trim().to_string()
    }
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Every path below a directory, with file contents or link targets.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let meta = fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() {
            found.push((path.clone(), Vec::new()));
            found.extend(snapshot(&path));
        } else if meta.is_symlink() {
            let target = fs::read_link(&path).unwrap();
            found.push((path, target.into_os_string().into_encoded_bytes()));
        } else {
            let contents = fs::read(&path).unwrap();
            found.push((path, contents));
        }
    }
    found.sort();
    found
}

fn backups(home: &Path) -> usize {
    let dir = home.join(".dotfiles-backup");
    if !dir.exists() {
        return 0;
    }
    snapshot(&dir).iter().filter(|(p, _)| !p.is_dir()).count()
}

#[test]
fn link_only_links_backs_up_and_is_idempotent() {
    let s = Sandbox::new();
    let home = s.home();
    fs::write(home.join(".zshrc"), "old zshrc\n").unwrap();
    fs::write(home.join(".gitconfig"), "[user]\n\tname = Old Name\n").unwrap();
    s.link();

    for file in [".zshrc", ".config/starship.toml"] {
        assert_eq!(
            fs::read_link(home.join(file)).unwrap(),
            s.repo().join("home").join(file)
        );
    }
    assert!(snapshot(&home.join(".dotfiles-backup"))
        .iter()
        .any(|(p, c)| p.ends_with(".zshrc") && c == b"old zshrc\n"));
    assert_eq!(s.identity(), "Old Name");
    let local = home.join(".zshrc.local");
    assert_eq!(fs::metadata(&local).unwrap().permissions().mode() & 0o777, 0o600);
    if cfg!(target_os = "linux") {
        assert!(!home.join("Library").exists(), "iTerm2 profile copied on Linux");
    }

    fs::write(&local, "keep me\n").unwrap();
    let before = snapshot(&home);
    s.link();
    assert_eq!(before, snapshot(&home), "second run changed something");
}

/// A copy that keeps links as links, like actions/checkout makes, must not be able to
/// write into the repo.
fn assert_gitconfig_isolated(s: &Sandbox, label: &str) {
    let gitconfig = s.home().join(".gitconfig");
    assert!(fs::symlink_metadata(&gitconfig).unwrap().is_file(), "{label}: it is a link");
    let checkout = s.root.path().join(format!("checkout-{label}"));
    fs::create_dir(&checkout).unwrap();
    let copied = Command::new("cp")
        .arg("-P")
        .arg(&gitconfig)
        .arg(checkout.join(".gitconfig"))
        .status()
        .unwrap();
    assert!(copied.success());
    let added = s
        .command("git", &checkout)
        .args(["config", "--global", "--add", "safe.directory", "/sim/repo"])
        .status()
        .unwrap();
    assert!(added.success());
    for file in [s.repo().join("home/.gitconfig"), gitconfig] {
        assert!(
            !fs::read_to_string(&file).unwrap().contains("/sim/repo"),
            "{label}: temporary write leaked into {}",
            file.display()
        );
    }
}

#[test]
fn gitconfig_stays_a_real_file_through_upgrades() {
    let s = Sandbox::new();
    let gitconfig = s.home().join(".gitconfig");
    fs::write(&gitconfig, "[user]\n\tname = Old Name\n").unwrap();
    s.link();
    assert_gitconfig_isolated(&s, "real-file");

    fs::remove_file(&gitconfig).unwrap();
    symlink(s.repo().join("home/.gitconfig"), &gitconfig).unwrap();
    s.link();
    assert_gitconfig_isolated(&s, "old-link");
    assert_eq!(s.identity(), "Old Name");
}

#[test]
fn a_managed_gitconfig_is_never_copied_to_the_local_file() {
    let s = Sandbox::new();
    let second = s.root.path().join("clone2");
    assert!(Command::new("cp").arg("-R").arg(s.repo()).arg(&second).status().unwrap().success());
    for repo in [s.repo(), s.repo(), second] {
        let out = s.rstuck_in(&repo, &s.home(), &["apply", "--link-only"]);
        assert!(out.status.success(), "{}", text(&out));
    }
    assert!(!s.home().join(".gitconfig.local").exists());
    let list = s
        .command("git", &s.home())
        .args(["config", "--global", "--includes", "--list"])
        .output()
        .unwrap();
    assert!(list.status.success(), "{}", text(&list));
}

#[test]
fn upgrading_from_install_sh_keeps_the_local_file_absent() {
    let s = Sandbox::new();
    let old = "# Managed by dotfiles install.sh. Untracked: git config --global writes here.\n\
         [include]\n\tpath = /somewhere/else/home/.gitconfig\n";
    fs::write(s.home().join(".gitconfig"), old).unwrap();
    s.link();
    assert!(!s.home().join(".gitconfig.local").exists());
}

#[test]
fn a_directory_linked_into_the_repo_is_left_alone() {
    let s = Sandbox::new();
    symlink(s.repo().join("home/.config"), s.home().join(".config")).unwrap();
    s.link();
    let tracked = s.repo().join("home/.config/starship.toml");
    assert!(fs::symlink_metadata(tracked).unwrap().is_file(), "repo file was replaced");
}

#[test]
fn dry_run_changes_nothing() {
    let s = Sandbox::new();
    fs::write(s.home().join(".zshrc"), "old zshrc\n").unwrap();
    fs::write(s.home().join(".gitconfig"), "[user]\n\tname = Old Name\n").unwrap();
    let before = snapshot(s.root.path());
    let out = s.ok(&["apply", "--link-only", "--dry-run"]);
    assert!(out.contains("would link"), "{out}");
    assert_eq!(before, snapshot(s.root.path()));
}

#[test]
fn status_reports_drift_in_config_files() {
    let s = Sandbox::new();
    let out = s.rstuck(&["status"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("config missing"), "{}", text(&out));
    s.link();
    // Packages and tools depend on the machine running the test; config must be clean.
    let after = text(&s.rstuck(&["status"]));
    for problem in ["config ", "file missing", "gitconfig", "copy out of date"] {
        assert!(!after.contains(problem), "{after}");
    }
}

#[test]
fn add_tracks_a_file_and_links_it() {
    let s = Sandbox::new();
    let file = s.home().join(".config/tmux/tmux.conf");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "set -g mouse on\n").unwrap();

    s.ok(&["add", ".config/tmux/tmux.conf", "--dry-run"]);
    let tracked = s.repo().join("home/.config/tmux/tmux.conf");
    assert!(!tracked.exists());

    s.ok(&["add", ".config/tmux/tmux.conf"]);
    assert_eq!(fs::read_to_string(&tracked).unwrap(), "set -g mouse on\n");
    assert_eq!(fs::read_link(&file).unwrap(), tracked);
    assert_eq!(backups(&s.home()), 1);
    assert!(s.ok(&["add", ".config/tmux/tmux.conf"]).contains("already tracked"));
}

#[test]
fn add_refuses_risky_files_and_changes_nothing() {
    let s = Sandbox::new();
    let home = s.home();
    fs::create_dir_all(home.join(".ssh")).unwrap();
    fs::write(home.join(".ssh/config"), "Host x\n").unwrap();
    fs::write(home.join(".zshrc.local"), "x\n").unwrap();
    fs::write(home.join(".vimrc"), "syntax on\n").unwrap();
    // Built at run time so this file passes the secret check itself.
    fs::write(home.join(".secretrc"), format!("export DEMO_API_{}=abcdefgh12345678\n", "KEY"))
        .unwrap();
    fs::write(home.join(".mailrc"), format!("from someone{}example.org\n", "@")).unwrap();
    fs::write(home.join(".binary"), b"a\0b").unwrap();
    fs::write(home.join(".zshrc"), "a different zshrc\n").unwrap();
    symlink(home.join(".ssh/config"), home.join(".sneaky")).unwrap();
    symlink(home.join(".ssh"), home.join("innocent")).unwrap();
    fs::write(s.root.path().join("outside"), "x\n").unwrap();

    let before = snapshot(s.root.path());
    for bad in [
        ".ssh/config", ".zshrc.local", ".secretrc", ".mailrc", ".binary", ".zshrc", ".sneaky",
        "innocent/config", "../outside", ".ssh", "no-such-file",
    ] {
        // A good file in the same command must not be added either.
        let out = s.rstuck(&["add", ".vimrc", bad]);
        assert!(!out.status.success(), "accepted {bad}: {}", text(&out));
        assert_eq!(before, snapshot(s.root.path()), "{bad} changed something");
    }
}

#[test]
fn pkg_add_edits_the_lists() {
    let s = Sandbox::new();
    s.ok(&["pkg", "add", "ripgrep", "--to", "common,brew"]);
    let listing = s.ok(&["pkg", "list"]);
    assert_eq!(listing.matches("ripgrep").count(), 2, "{listing}");
    assert!(!s.rstuck(&["pkg", "add", "zsh-", "--to", "apt"]).status.success());
    assert!(!s.rstuck(&["pkg", "add", "vim"]).status.success(), "--to is required");
}

#[test]
fn hook_is_installed_as_a_copy() {
    let s = Sandbox::new();
    let git = |args: &[&str]| {
        s.command("git", &s.home())
            .arg("-C")
            .arg(s.repo())
            .args(args)
            .output()
            .unwrap()
    };
    assert!(git(&["init", "-q"]).status.success());
    assert!(git(&["config", "core.hooksPath", ".githooks"]).status.success());
    s.link();
    let hook = s.repo().join(".git/hooks/pre-commit");
    assert_eq!(
        fs::read(&hook).unwrap(),
        fs::read(s.repo().join(".githooks/pre-commit")).unwrap()
    );
    assert!(fs::symlink_metadata(&hook).unwrap().is_file());
    assert_ne!(fs::metadata(&hook).unwrap().permissions().mode() & 0o111, 0);
    assert!(!git(&["config", "--local", "core.hooksPath"]).status.success());
}

#[test]
fn a_broken_manifest_stops_before_any_change() {
    let s = Sandbox::new();
    fs::write(s.repo().join("rstuck.toml"), "[[tools]]\n").unwrap();
    let before = snapshot(&s.home());
    assert!(!s.rstuck(&["apply", "--link-only"]).status.success());
    assert_eq!(before, snapshot(&s.home()));
}
