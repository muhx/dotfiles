# dotfiles

My macOS terminal setup: zsh + antidote plugins, Starship, fzf, zoxide, Fira Code and an iTerm2 profile.

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

Keep the clone where it is: files in `$HOME` are symlinks into it. The exception is `~/.gitconfig`, a real untracked file that includes `home/.gitconfig`. Tools that run `git config --global` write there, not into this repo. A symlink would not do: `actions/checkout` on a self-hosted runner copies it as a symlink and writes through it. Re-running `install.sh` is safe. Replaced files are moved to `~/.dotfiles-backup/`. Use `--link-only` to skip package and plugin setup.

On a machine that already has its own `.zshrc`, move any secrets and custom lines you need from the backup into `~/.zshrc.local`.

## Sync changes

Linked files change in the repo as soon as you edit them. For iTerm2, run `./export-iterm.sh` first. It exports only allowlisted settings. It keeps the committed Triggers and Keyboard Map unless you review yours and pass `--include-bindings`.

```sh
cd ~/dotfiles && git diff     # review: this repo is public
git switch -c chore/update-config && git commit -am "update config" && git push -u origin HEAD
```

On the other Mac: `git pull && ./install.sh`.

## Secrets

This repo is **public**. Keys, tokens, email and per-machine paths belong in `~/.zshrc.local` or `~/.gitconfig.local`, which are not tracked. Some installers append lines to `~/.zshrc`, which is the tracked file, so check the diff. `install.sh` copies a pre-commit hook into `.git/hooks` that blocks common secret patterns and email addresses. Re-run it to update the hook. It is a safety net, not a guarantee.

Do not run scripts from a pull request you have not reviewed.

## Not included

SSH and GPG keys, `gh` login, shell history, and app or agent configs.

`./test.sh` checks the installer against a temporary home directory.
