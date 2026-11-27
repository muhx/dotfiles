# dotfiles

My terminal setup: zsh + antidote plugins, Starship, fzf, zoxide, plus Fira Code and an iTerm2 profile on macOS. Works on Mac and on Linux servers (Ubuntu, Debian, Rocky Linux).

## Install on a new Mac

1. Install [Homebrew](https://brew.sh) (it also installs git).
2. Run:

```sh
git clone https://github.com/muhx/dotfiles.git ~/dotfiles
~/dotfiles/install.sh
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
~/dotfiles/install.sh
```

Then do step 3 above and log in again. What the script does on Linux:

- Installs the [system packages](#packages) with apt or dnf. On Rocky it enables EPEL first, for htop. It adds GitHub's apt or dnf repo for `gh`.
- Installs antidote, fzf, Starship and zoxide for your user under `~/.local`, because distro versions are often too old (`fzf --zsh` needs 0.48+).
- Installs Rust with rustup into `~/.cargo` to build the alias-tips helper, unless cargo 1.85+ is already there.
- Changes your login shell to zsh.

With no sudo, ask an admin for `zsh git curl tar gcc` and run `./install.sh --no-root`, which skips the system packages and the shell change. iTerm2, fonts and the `macos` zsh plugin are skipped on Linux. The font belongs to the terminal you connect from.

Keep the clone where it is: files in `$HOME` are symlinks into it. The exception is `~/.gitconfig`, a real untracked file that includes `home/.gitconfig`. Tools that run `git config --global` write there, not into this repo. A symlink would not do: `actions/checkout` on a self-hosted runner copies it as a symlink and writes through it. Re-running `install.sh` is safe. Replaced files are moved to `~/.dotfiles-backup/`. Use `--link-only` to skip package and plugin setup.

On a machine that already has its own `.zshrc`, move any secrets and custom lines you need from the backup into `~/.zshrc.local`.

## Packages

| File | Installed on |
|---|---|
| `packages/common.txt` | all Linux servers |
| `packages/apt.txt` | Ubuntu and Debian only |
| `packages/dnf.txt` | Rocky and other dnf distros only |
| `Brewfile` | macOS |

To add a tool, put its name in `common.txt` if apt and dnf call it the same, or in `apt.txt` and `dnf.txt` if the names differ. For macOS, also add it to the `Brewfile`. The lists take one package name per line, and `#` starts a comment. Then run `./install.sh` (not `--link-only` or `--no-root`, which skip these packages).

`install.sh` handles `gh` (it needs GitHub's repo) and `curl` on dnf distros (added only if missing, because Rocky's `curl-minimal` conflicts with it). Keep both out of `common.txt` and `dnf.txt`. Removing a name does not uninstall the package.

To add a config file, put it under `home/` at the path it has under `$HOME`, then run `./install.sh --link-only`.

## Sync changes

Linked files change in the repo as soon as you edit them. For iTerm2, run `./export-iterm.sh` first. It exports only allowlisted settings. It keeps the committed Triggers and Keyboard Map unless you review yours and pass `--include-bindings`.

```sh
cd ~/dotfiles && git diff     # review: this repo is public
git switch -c chore/update-config && git commit -am "update config" && git push -u origin HEAD
```

On the other machine: `git pull && ./install.sh`.

## Secrets

This repo is **public**. Keys, tokens, email and per-machine paths belong in `~/.zshrc.local` or `~/.gitconfig.local`, which are not tracked. Some installers append lines to `~/.zshrc`, which is the tracked file, so check the diff. `install.sh` copies a pre-commit hook into `.git/hooks` that blocks common secret patterns and email addresses. Re-run it to update the hook. It is a safety net, not a guarantee.

Do not run scripts from a pull request you have not reviewed.

## Not included

SSH and GPG keys, `gh` login, shell history, and app or agent configs.

`./test.sh` checks the installer against a temporary home directory. It also checks the package lists against stubbed apt and dnf commands, so it installs nothing.
