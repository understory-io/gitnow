# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `gitnow project create --allow-empty` creates a project with no repositories, skipping repo selection. Without it, an automated caller that doesn't yet know which repos it needs has no way through: omitting `--repos` opens the fzf picker, which needs a TTY. Repos can be added later with `gitnow project add`

## [0.7.0] - 2026-09-10

### Added
- `gitnow list` prints the known repository set non-interactively, with `--json`, `--cloned` and query filtering
- work with zero configuration on a fresh install: when no providers are configured, gitnow sniffs `gh auth token` (falling back to `GH_TOKEN`/`GITHUB_TOKEN`) and the `gh` login to index your own GitHub. Configuring any provider disables the sniff, and the token is never persisted to the config file nor printed

### Fixed
- expand `~` in `settings.projects.directory` and `settings.cache.location`; both were used verbatim, so a `~/git` config cloned into a literal `./~/git` relative to the current directory

## [0.6.0] - 2026-09-07

### Added
- support multi-term, fzf-style repository queries and pre-filtered interactive selection
- add the `gi` zsh helper for interactive repository navigation

### Fixed
- preserve duplicate picker entries and handle Unicode backspace safely

## [0.5.1] - 2026-08-14

### Fixed
- delete up to four projects concurrently so automatic retention and bulk deletion no longer serialize filesystem cleanup

## [0.5.0] - 2026-08-14

### Added
- filter bulk project deletion by age or cutoff date, preview matches unless `--quiet` is set, and support configurable automatic retention

## [0.3.5] - 2025-03-04

### Added
- allow clone all

### Fixed
- *(deps)* update rust crate serde to v1.0.218
- *(deps)* update tokio-prost monorepo to v0.13.5
- *(deps)* update rust crate termwiz to 0.23.0
- *(deps)* update all dependencies
- *(deps)* update rust crate uuid to v1.13.0
- *(deps)* update all dependencies
- *(deps)* update rust crate async-trait to v0.1.86
- *(deps)* update rust crate uuid to v1.12.1
- *(deps)* update rust crate octocrab to 0.43.0
- *(deps)* update rust crate uuid to v1.12.0
- *(deps)* update rust crate dirs to v6
- *(deps)* update rust crate uuid to v1.11.1

### Other
- *(deps)* update all dependencies
- *(deps)* update all dependencies
- *(deps)* update rust crate clap to v4.5.29
- *(deps)* update rust crate clap to v4.5.27
- *(deps)* update rust crate clap to v4.5.26
- *(deps)* update rust crate tokio to v1.43.0
- update example

## [0.3.4] - 2025-01-08

### Added
- feat/add-post-clone-command

- add ability to specify custom command

### Fixed
- use correct post clone command
- tests for config

### Other
- add ability to specify multiple commands


## [0.3.3] - 2025-01-07

### Other
- replace dotenv with dotenvy, a slightly more maintained version of the same library (#50)

## [0.3.2] - 2025-01-07

### Fixed
- *(deps)* update rust crate async-trait to v0.1.85

## [0.3.1] - 2025-01-02

### Other
- enable publish

## [0.3.0] - 2025-01-01

### Added
- add small help to see how much time is left in cache

### Fixed
- *(deps)* update rust crate serde to v1.0.217
- *(deps)* update rust crate serde to v1.0.216
- *(deps)* update tokio-prost monorepo to v0.13.4
- *(deps)* update rust crate bytes to v1.9.0
- *(deps)* update all dependencies
- *(deps)* update rust crate octocrab to 0.42.0
- *(deps)* update rust crate serde to v1.0.215
- *(deps)* update rust crate url to v2.5.3
- *(deps)* update rust crate serde to v1.0.214
- *(deps)* update rust crate serde to v1.0.213
- *(deps)* update all dependencies
- *(deps)* update all dependencies
- *(deps)* update rust crate octocrab to v0.41.1
- *(deps)* update rust crate futures to v0.3.31
- *(deps)* update rust crate octocrab to 0.41.0

### Other
- bump default cache duration to 7 days
- *(deps)* update rust crate anyhow to v1.0.95
- *(deps)* update rust crate clap to v4.5.23
- *(deps)* update all dependencies
- *(deps)* update rust crate tracing-subscriber to v0.3.19
- *(deps)* update rust crate tracing to v0.1.41
- *(deps)* update rust crate clap to v4.5.21
- *(deps)* update rust crate tokio to v1.41.1
- *(deps)* update rust crate anyhow to v1.0.93
- *(deps)* update rust crate anyhow to v1.0.92
- *(deps)* update all dependencies to v1.0.91
- *(deps)* update rust crate anyhow to v1.0.90
- *(deps)* update rust crate clap to v4.5.20
- *(deps)* update rust crate clap to v4.5.19

## [0.2.3] - 2024-09-26

### Added
- add update command
- only do clone if not exists

### Fixed
- *(deps)* update rust crate async-trait to v0.1.83
- *(deps)* update rust crate octocrab to 0.40.0

## [0.2.2] - 2024-09-23

### Other
- add docs

main@origin

- add license
- update to gitea-client
- add publish
- *(release)* 0.2.1

## [0.2.1] - 2024-09-23

### Added
- use termwiz as backend as that enables a ptty, which can be cleaned up nicely
- add errout for interactive for script support and atty for clean output
- add clone spinner
- add spinner around download
- spawn a subshell for session
- implement git clone
- include vhs demo
- add interactive search
- implement naive fuzzy matcher

### Fixed
- *(deps)* update tokio-prost monorepo to v0.13.3
- *(deps)* update rust crate bytes to v1.7.2

### Other
- update gif to include spinner
- clean up ui
- build in cuddle instead of vhs
- build first then run
- clear screen after build
- fix warnings
- update theme for vhs
- *(deps)* update rust crate clap to v4.5.18
- *(deps)* update rust crate pretty_assertions to v1.4.1
- refactor fuzzy match into own function
- cleanup warnings
- move fuzzy search out of command
- refactor/matcher move to a separate file

- move fuzzy search out of command
- Actually add fuzzy matcher

- extract matcher
- update dependencies
- *(deps)* update rust crate anyhow to v1.0.89

## [0.2.0] - 2024-09-14

### Added
- add cache get
- send out wait
- add cache
- add settings config
- add github fetch prs refactoring
- gitea able to pull repositories
- add config

### Docs
- add readme

### Fixed
- don't have to use user for basic auth

### Other
- removed unused code
- move projects list into separate file
- separate files
- move config out
- remove unused libraries

## [0.1.0] - 2024-09-12

### Added
- init

### Docs
- test
