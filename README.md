# dotfiles

My productivity setup, managed by `rstuck`, a small Rust CLI in this repo. It covers zsh + antidote plugins, Starship, fzf, zoxide, plus Fira Code and an iTerm2 profile on macOS. Works on Mac and on Linux servers (Ubuntu, Debian, Rocky Linux).

## Install on a new Mac

1. Install [Homebrew](https://brew.sh) (it also installs git).
2. Run:

```sh
git clone https://github.com/muhx/dotfiles.git ~/dotfiles
~/dotfiles/bootstrap.sh
```

3. Add your git identity (and signing key, if this machine has one):

```sh
git config -f ~/.gitconfig.local user.name "Your Name"
git config -f ~/.gitconfig.local user.email "you@example.com"
# git config -f ~/.gitconfig.local user.signingkey <KEY_ID>
# git config -f ~/.gitconfig.local commit.gpgsign true
gh auth login
```

4. In iTerm2: Settings > Profiles > "Source Mac" > Other Actions > Set as Default. Open a new window.

## Install on a Linux server

Ubuntu, Debian and Rocky Linux are supported. Other apt or dnf distros (Alma, CentOS Stream, Fedora) should work too. You need `git` and `sudo` (or root):

```sh
sudo apt-get install -y git    # Rocky: sudo dnf install -y git
git clone https://github.com/muhx/dotfiles.git ~/dotfiles
~/dotfiles/bootstrap.sh
```

`bootstrap.sh` makes sure git, curl, a C compiler and Rust 1.85+ are there (installing Rust with rustup, or with Homebrew on macOS), builds `rstuck` into `~/.local/bin`, then runs `rstuck apply`. The first build downloads Rust crates and takes a while.

Then do step 3 above and log in again. What `rstuck apply` does on Linux:

- Installs the [system packages](#packages) with apt or dnf. On Rocky it enables EPEL first, for htop. It adds GitHub's apt or dnf repo for `gh`.
- Installs the tools in [`rstuck.toml`](rstuck.toml) (antidote, fzf, Starship and zoxide) for your user under `~/.local`, because distro versions are often too old (`fzf --zsh` needs 0.48+).
- Installs Rust with rustup into `~/.cargo` to build the alias-tips helper, unless cargo 1.85+ is already there.
- Changes your login shell to zsh.

With no sudo, ask an admin for `zsh git curl tar gcc` (and the C library headers, `libc6-dev` or `glibc-devel`) and run `./bootstrap.sh --no-root`, which skips the system packages and the shell change. iTerm2, fonts and the `macos` zsh plugin are skipped on Linux. The font belongs to the terminal you connect from.

Keep the clone where it is: files in `$HOME` are symlinks into it. The exception is `~/.gitconfig`, a real untracked file that includes `home/.gitconfig`. Tools that run `git config --global` write there, not into this repo. A symlink would not do: `actions/checkout` on a self-hosted runner copies it as a symlink and writes through it. Re-running `rstuck apply` is safe. Replaced files are moved to `~/.dotfiles-backup/`. Use `rstuck apply --link-only` to skip package, tool and plugin setup.

On a machine that already has its own `.zshrc`, move any secrets and custom lines you need from the backup into `~/.zshrc.local`.

## Commands

| Command | What it does |
|---|---|
| `rstuck apply` | Installs packages and tools, links config files, sets up zsh. Takes `--dry-run`, `--link-only` and `--no-root`. |
| `rstuck status` | Lists what differs from the repo: system packages, tools, config links, seed and copied files, the git hook. Exit code 1 if anything does. It does not check zsh plugins, the login shell or versions. |
| `rstuck add <file>...` | Starts tracking config files: copies them into `home/`, then links them. Takes `--dry-run`. |
| `rstuck pkg add <name>... --to <list>` | Adds packages to `common`, `apt`, `dnf` or `brew` (several: `--to common,brew`; `--cask` for casks). |
| `rstuck pkg list` | Prints the package lists. |

`rstuck` uses the clone it was built from. To use another, pass `--repo <dir>` or set `RSTUCK_REPO`.

## Add to your setup

None of these need Rust changes. They edit the repo only, so commit them to share them.

**A config file:**

```sh
rstuck add ~/.config/tmux/tmux.conf
```

`add` refuses links, binary files, files outside your home directory, paths that usually hold credentials (such as `~/.ssh` or `*.local`), and anything the secret check rejects. If any file is refused, none are added. These checks cannot catch every secret, so read the file before you commit it.

**A package:**

```sh
rstuck pkg add ripgrep --to common,brew
rstuck apply
```

| File | Installed on |
|---|---|
| `packages/common.txt` | all Linux servers (same name on apt and dnf) |
| `packages/apt.txt` | Ubuntu and Debian only |
| `packages/dnf.txt` | Rocky and other dnf distros only |
| `Brewfile` | macOS |

The Linux lists take one package name per line, and `#` starts a comment. `--link-only` and `--no-root` skip system packages. `rstuck` handles `gh` (it needs GitHub's repo) and `curl` on dnf distros (added only if missing, because Rocky's `curl-minimal` conflicts with it). Removing a name does not uninstall the package.

**A tool that is not a system package, or a file to copy:** edit [`rstuck.toml`](rstuck.toml). It explains `[[tool]]`, `[[seed]]` and `[[copy]]`. A tool's `run` script is skipped once the tool is there, so this does not upgrade tools.

## Sync changes

Linked files change in the repo as soon as you edit them. For iTerm2, run `./export-iterm.sh` first. It exports only allowlisted settings. It keeps the committed Triggers and Keyboard Map unless you review yours and pass `--include-bindings`.

```sh
cd ~/dotfiles && git status && git diff   # review: this repo is public
git switch -c chore/update-config
git add <files>                           # includes new files from `rstuck add`
git commit -m "update config" && git push -u origin HEAD
```

On the other machine: `git pull && ./bootstrap.sh`. This also rebuilds `rstuck` if its code changed.

## Secrets

This repo is **public**. Keys, tokens, email and per-machine paths belong in `~/.zshrc.local` or `~/.gitconfig.local`, which are not tracked. Some installers append lines to `~/.zshrc`, which is the tracked file, so check the diff. `rstuck apply` copies a pre-commit hook into `.git/hooks` that blocks common secret patterns and email addresses. Re-run it to update the hook. It is a safety net, not a guarantee.

Do not run scripts from a pull request you have not reviewed.

## Not included

SSH and GPG keys, `gh` login, shell history, and app or agent configs.

## Development

`./test.sh` runs `cargo test` (unit tests, plus end-to-end tests against a temporary home directory), then checks the secret hook, the iTerm2 export and zsh syntax. It installs no system packages and does not touch your home directory. The first run downloads Rust crates.
