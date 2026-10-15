# Git Now

> https://gitnow.kjuulh.io/

Git Now is a utility for easily navigating git projects from common upstream providers. Search, Download, and Enter projects as quickly as you can type.

![example gif](./assets/gifs/example.gif)

## Installation

### Homebrew

```bash
brew tap kjuulh/tap https://git.kjuulh.io/kjuulh/homebrew-tap
brew install gitnow
```

### Cargo

```bash
cargo install gitnow
# or
cargo binstall gitnow
```

### Setup

```bash
# You can either use gitnow directly (and use spawned shell sessions)
gitnow

# Or install gitnow scripts (in your .bashrc, .zshrc) this will use native shell commands to move you around
eval "$(gitnow init zsh)"
git-now mire api # Jump to the best match (long form)
gn mire api      # Jump to the best match (short alias)
gi mire api      # Interactively choose from the filtered matches
```

## Reasoning

How many steps do you normally do to download a project?

1. Navigate to github.com
2. Search in your org for the project
3. Find the clone url
4. Navigate to your local github repositories path
5. Git clone `<project>` 
6. Enter new project directory

A power user can of course use `gh repo clone` to skip a few steps.

With gitnow

1. `git now`
2. Enter parts of the project name and press enter
3. Your project is automatically downloaded if it doesn't exist in an opinionated path dir, and move you there.

Queries accept multiple space-separated terms; every term must match. The
fzf-style operators `'exact`, `^prefix`, `suffix$`, and `!exclude` are also
supported. Quote operators that your shell would otherwise interpret:

```bash
gitnow mire api
gitnow "'gitnow" "'kjuulh"
gitnow gitnow '!github.com'
gitnow --interactive mire
```

## Configuration

Configuration lives at `~/.config/gitnow/gitnow.toml` (override with `$GITNOW_CONFIG`).

### Zero-config first run

A fresh install needs no configuration. When gitnow finds **no providers
configured at all** — no config file, an empty one, or one with only
`[settings]` — it sniffs your environment for GitHub credentials and indexes
**your own GitHub** with them:

| Step | Source | Fallback |
| ---- | ------ | -------- |
| Token | `gh auth token` | `GH_TOKEN`, then `GITHUB_TOKEN` |
| Login | `gh api user --jq .login` | none needed — the token already scopes `/user/repos` |
| Host | `github.com` | — |

So on a machine where you have already run `gh auth login`, this just works:

```bash
gitnow            # search, clone and enter one of your repositories
```

Notes:

- **Explicit config always wins.** Configuring *any* provider — GitHub or
  Gitea — turns the sniff off entirely; your providers are used exactly as
  written, and gitnow never shells out to `gh`. Customising `[settings]` alone
  does *not* turn it off, since the trigger is about providers.
- **The token is never persisted.** The synthesised provider only ever exists
  in memory: nothing is written to your config file, and the token is
  re-resolved on each run. Environment tokens are referenced by variable name
  rather than by value, and tokens are redacted from all debug output.
- **`gh` is optional.** It is a best-effort shell-out with a timeout, run
  lazily (only for commands that list repositories, and at most once per run).
  If neither `gh` nor a token variable is available, gitnow prints a one-line
  hint and carries on — it never prompts, so automation is unaffected.
- Organisations are not seeded: `/user/repos` already covers the organisation
  repositories your token can see.

Writing a real config later needs no migration — it simply takes over.

### Custom clone command

By default gitnow uses `git clone`. You can override this with any command using a [minijinja](https://docs.rs/minijinja) template:

```toml
[settings]
# Use jj (Jujutsu) instead of git
clone_command = "jj git clone {{ ssh_url }} {{ path }}"
```

Available template variables: `ssh_url`, `clone_url`, `path`. Use `clone_url`
for the provider's HTTPS clone URL:

```toml
[settings]
clone_command = "git clone {{ clone_url }} {{ path }}"
```

### Worktrees

gitnow supports git worktrees (or jj workspaces) via the `worktree` subcommand. This uses bare repositories so each branch gets its own directory as a sibling:

```
~/git/github.com/owner/repo/
├── .bare/          # bare clone (git clone --bare)
├── main/           # worktree for main branch
├── feature-login/  # worktree for feature/login branch
└── fix-typo/       # worktree for fix/typo branch
```

Usage:

```bash
# Interactive: pick repo, then pick branch
gitnow worktree

# Pre-filter repo
gitnow worktree myproject

# Specify branch directly
gitnow worktree myproject -b feature/login

# Print worktree path instead of entering a shell
gitnow worktree myproject -b main --no-shell
```

All worktree commands are configurable via minijinja templates:

```toml
[settings.worktree]
# Default: "git clone --bare {{ ssh_url }} {{ bare_path }}"
clone_command = "git clone --bare {{ ssh_url }} {{ bare_path }}"

# Default: "git -C {{ bare_path }} worktree add {{ worktree_path }} {{ branch }}"
add_command = "git -C {{ bare_path }} worktree add {{ worktree_path }} {{ branch }}"

# Default: "git -C {{ bare_path }} branch --format=%(refname:short)"
list_branches_command = "git -C {{ bare_path }} branch --format=%(refname:short)"
```

For jj, you might use:

```toml
[settings]
clone_command = "jj git clone {{ ssh_url }} {{ path }}"

[settings.worktree]
clone_command = "jj git clone {{ ssh_url }} {{ bare_path }}"
add_command = "jj -R {{ bare_path }} workspace add --name {{ branch }} {{ worktree_path }}"
list_branches_command = "jj -R {{ bare_path }} bookmark list -T 'name ++ \"\\n\"'"
```

Available template variables for worktree commands: `bare_path`, `worktree_path`, `branch`, `ssh_url`, `clone_url`.

### Listing repositories

`gitnow list` prints the known repository set and exits — no picker, no clone, no
sub-shell. Use it from scripts, or to drive completion in another tool.

```bash
# Every known repository, one relative path per line
gitnow list

# Filtered with the same fzf-style query as the default command
gitnow list understory-io

# Only the ones already cloned
gitnow list --cloned

# JSON, with ssh_url / path / cloned per repo
gitnow list --json | jq -r '.[] | select(.cloned == false) | .ssh_url'
```

### Projects

gitnow supports scratch-pad projects that group multiple repositories into a single directory. This is useful when working on features that span several repos.

```bash
# Create a new project (interactive repo selection)
gitnow project create my-feature

# Create from a template
gitnow project create my-feature -t default

# Create non-interactively with specific repos (fuzzy-matched)
gitnow project create my-feature --repos repo-a --repos repo-b --no-template --no-shell

# Open an existing project (interactive selection)
gitnow project

# Open by name
gitnow project my-feature

# List all projects
gitnow project list

# List with repo details
gitnow project list --repos

# List as JSON (for scripting)
gitnow project list --repos --json

# Add more repos to a project (interactive)
gitnow project add my-feature

# Add repos non-interactively
gitnow project add my-feature --repos repo-c --repos repo-d

# Delete a project
gitnow project delete my-feature

# Delete without confirmation
gitnow project delete my-feature --force

# Delete projects created before a date (midnight UTC)
gitnow project delete --before 2026-07-15

# Delete projects older than 30 days without confirmation
gitnow project delete --older-than 30 --force

# Suppress the preview and success output for automation
gitnow project delete --older-than 30 --force --quiet
```

Deletion commands preview every selected project unless `--quiet` is set. `--force` skips the confirmation prompt but still shows the preview. `--before` accepts either `YYYY-MM-DD` (midnight UTC) or an RFC 3339 timestamp.

Project directories live at `~/.gitnow/projects/` by default. Templates live at `~/.gitnow/templates/`. Both are configurable:

```toml
[settings.project]
directory = "~/.gitnow/projects"
templates_directory = "~/.gitnow/templates"
auto_delete_older_than_days = 30
```

When `auto_delete_older_than_days` is set, each `gitnow project` command except an explicit `project delete` first removes metadata-backed projects older than the configured retention period. Automatic cleanup is non-interactive and prints the projects it removes.

Commands that navigate to a directory (`gitnow`, `gitnow project`, `gitnow project create`, `gitnow worktree`) will `cd` you there when using the shell integration. Commands that don't produce a path (`project add`, `project delete`, `update`) run normally without changing your directory.

### Shell integration

The recommended way to use gitnow is with shell integration, which uses a **chooser file** to communicate the selected path back to your shell:

```bash
eval "$(gitnow init zsh)"
git-now mire    # best match
gn mire         # short alias for git-now
gi mire         # interactive picker pre-filtered with "mire"
```

When you run `git-now`, the shell wrapper:

1. Creates a temporary chooser file
2. Runs `gitnow` with the `GITNOW_CHOOSER_FILE` env var pointing to it
3. If gitnow writes a path to the file, the wrapper `cd`s there
4. If the file is empty (e.g. after `git-now project delete`), no `cd` happens

This works uniformly for all subcommands:

```bash
git-now mire api             # jump directly to the best multi-term match
gi mire api                  # choose interactively from those matches
git-now project              # pick a project and cd there
git-now project create foo   # create project and cd there
git-now project delete foo   # deletes project, no cd
git-now worktree             # pick repo+branch worktree, cd there
```

You can also set the chooser file manually for scripting:

```bash
GITNOW_CHOOSER_FILE=/tmp/choice gitnow project
# or
gitnow --chooser-file /tmp/choice project
```
