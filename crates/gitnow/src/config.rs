use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Expand a leading `~` to the home directory.
///
/// `toml` deserializes a path into a `PathBuf` verbatim, and nothing downstream resolves it:
/// a configured `~/git` stays a *relative* path whose first component is literally `~`, so it
/// resolves against the current directory instead of $HOME. Only `~` and `~/...` are handled,
/// not `~user/...`, which no shell-less consumer of this config can resolve anyway.
pub(crate) fn expand_tilde(path: PathBuf) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        Err(_) => path,
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,

    #[serde(default)]
    pub providers: Providers,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Settings {
    #[serde(default)]
    pub projects: Projects,

    #[serde(default)]
    pub cache: Cache,

    pub post_clone_command: Option<PostCloneCommand>,
    pub post_update_command: Option<PostUpdateCommand>,
    #[serde(default)]
    pub post_update_command_by_vcs: PostUpdateCommandsByVcs,

    /// Minijinja template for the clone command.
    /// Default: "git clone {{ ssh_url }} {{ path }}"
    pub clone_command: Option<String>,

    /// Worktree configuration.
    #[serde(default)]
    pub worktree: Option<WorktreeSettings>,

    /// Project scratch-pad configuration.
    #[serde(default)]
    pub project: Option<ProjectSettings>,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct PostUpdateCommandsByVcs {
    pub git: Option<CommandList>,
    pub jj: Option<CommandList>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ProjectSettings {
    /// Directory where projects are stored.
    /// Default: "~/.gitnow/projects"
    pub directory: Option<String>,

    /// Directory containing project templates.
    /// Each subdirectory is a template whose files are copied into new projects.
    /// Default: "~/.gitnow/templates"
    pub templates_directory: Option<String>,

    /// Automatically delete projects older than this many days whenever a
    /// project command runs.
    pub auto_delete_older_than_days: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct WorktreeSettings {
    /// Template for bare-cloning a repository.
    /// Default: "git clone --bare {{ ssh_url }} {{ bare_path }}"
    pub clone_command: Option<String>,

    /// Template for adding a worktree.
    /// Default: "git -C {{ bare_path }} worktree add {{ worktree_path }} {{ branch }}"
    pub add_command: Option<String>,

    /// Template for listing branches.
    /// Default: "git -C {{ bare_path }} branch -r --format=%(refname:short)"
    pub list_branches_command: Option<String>,
}

/// A list of shell commands that can be specified as a single string or an array in TOML.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(untagged)]
pub enum CommandList {
    Single(String),
    Multiple(Vec<String>),
}

impl CommandList {
    pub fn get_commands(&self) -> Vec<String> {
        match self.clone() {
            CommandList::Single(item) => vec![item],
            CommandList::Multiple(items) => items,
        }
    }
}

/// Backwards-compatible type aliases.
pub type PostCloneCommand = CommandList;
pub type PostUpdateCommand = CommandList;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Projects {
    pub directory: ProjectLocation,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectLocation(PathBuf);

impl<'de> Deserialize<'de> for ProjectLocation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(expand_tilde(PathBuf::deserialize(deserializer)?)))
    }
}

impl From<PathBuf> for ProjectLocation {
    fn from(value: PathBuf) -> Self {
        Self(value)
    }
}

impl From<ProjectLocation> for PathBuf {
    fn from(value: ProjectLocation) -> Self {
        value.0
    }
}

impl Default for ProjectLocation {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_default();

        Self(home.join("git"))
    }
}

impl std::ops::Deref for ProjectLocation {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Cache {
    #[serde(default)]
    pub location: CacheLocation,

    #[serde(default)]
    pub duration: CacheDuration,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CacheLocation(PathBuf);

impl<'de> Deserialize<'de> for CacheLocation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(expand_tilde(PathBuf::deserialize(deserializer)?)))
    }
}

impl From<PathBuf> for CacheLocation {
    fn from(value: PathBuf) -> Self {
        Self(value)
    }
}

impl From<CacheLocation> for PathBuf {
    fn from(value: CacheLocation) -> Self {
        value.0
    }
}

impl Default for CacheLocation {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_default();

        Self(home.join(".cache/gitnow"))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(untagged)]
pub enum CacheDuration {
    Enabled(bool),
    Precise {
        #[serde(default)]
        days: u64,
        #[serde(default)]
        hours: u64,
        #[serde(default)]
        minutes: u64,
    },
}

impl CacheDuration {
    pub fn get_duration(&self) -> Option<std::time::Duration> {
        match self {
            CacheDuration::Enabled(true) => CacheDuration::default().get_duration(),
            CacheDuration::Enabled(false) => None,
            CacheDuration::Precise {
                days,
                hours,
                minutes,
            } => Some(
                std::time::Duration::from_hours(*days * 24)
                    + std::time::Duration::from_hours(*hours)
                    + std::time::Duration::from_mins(*minutes),
            ),
        }
    }
}

impl Default for CacheDuration {
    fn default() -> Self {
        Self::Precise {
            days: 7,
            hours: 0,
            minutes: 0,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Providers {
    #[serde(default)]
    pub github: Vec<GitHub>,
    #[serde(default)]
    pub gitea: Vec<Gitea>,
}

impl Providers {
    /// True when no provider at all is configured — the fresh-install case
    /// where the config file is absent, empty, or declares only `[settings]`.
    ///
    /// This is the sole trigger for the zero-config default provider (see
    /// [`crate::zero_config`]).  It looks at *providers* only: customising
    /// settings does not turn the default off, and configuring any provider —
    /// GitHub or Gitea — does.
    pub fn is_empty(&self) -> bool {
        self.github.is_empty() && self.gitea.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct GitHub {
    #[serde(default)]
    pub url: Option<String>,

    pub access_token: GitHubAccessToken,

    #[serde(default)]
    pub current_user: Option<String>,

    #[serde(default)]
    pub users: Vec<GitHubUser>,
    #[serde(default)]
    pub organisations: Vec<GitHubOrganisation>,
}

/// Generates a newtype wrapper around `String` with `From` impls for owned and borrowed access.
macro_rules! string_newtype {
    ($name:ident) => {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
        pub struct $name(String);

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl<'a> From<&'a $name> for &'a str {
            fn from(value: &'a $name) -> Self {
                value.0.as_str()
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

string_newtype!(GitHubUser);
string_newtype!(GitHubOrganisation);

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Gitea {
    pub url: String,

    #[serde(default)]
    pub access_token: Option<GiteaAccessToken>,

    #[serde(default)]
    pub current_user: Option<String>,

    #[serde(default)]
    pub users: Vec<GiteaUser>,
    #[serde(default)]
    pub organisations: Vec<GiteaOrganisation>,
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(untagged)]
pub enum GiteaAccessToken {
    Direct(String),
    Env { env: String },
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(untagged)]
pub enum GitHubAccessToken {
    Direct(String),
    Env { env: String },
}

/// Redacts the literal token, so it cannot reach a log line, a panic message,
/// or `--verbose` output by way of a `Debug` impl further up the tree.
macro_rules! redacted_debug {
    ($name:ident) => {
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    Self::Direct(_) => f.write_str("Direct(<redacted>)"),
                    Self::Env { env } => f.debug_struct("Env").field("env", env).finish(),
                }
            }
        }
    };
}

redacted_debug!(GiteaAccessToken);
redacted_debug!(GitHubAccessToken);

string_newtype!(GiteaUser);
string_newtype!(GiteaOrganisation);

impl Config {
    pub async fn from_file(file_path: &Path) -> anyhow::Result<Config> {
        if !file_path.exists() {
            if let Some(parent) = file_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::File::create(file_path).await?;
        }

        let file_content = tokio::fs::read_to_string(file_path).await?;

        Self::from_string(&file_content)
    }

    pub fn from_string(content: &str) -> anyhow::Result<Config> {
        toml::from_str(content).context("failed to deserialize config file")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn expands_tilde_in_configured_locations() -> anyhow::Result<()> {
        let home = dirs::home_dir().unwrap_or_default();
        let config = Config::from_string(
            r#"
              [settings]
              projects = { directory = "~/git" }

              [settings.cache]
              location = "~/.cache/gitnow"
            "#,
        )?;

        // A literal `~` first component would resolve against the current directory, so
        // these paths must come out absolute or the clone lands in `./~/git/...`.
        assert_eq!(*config.settings.projects.directory, home.join("git"));
        assert_eq!(
            PathBuf::from(config.settings.cache.location.clone()),
            home.join(".cache/gitnow")
        );

        Ok(())
    }

    #[test]
    fn leaves_absolute_locations_alone() -> anyhow::Result<()> {
        let config = Config::from_string(
            r#"
              [settings]
              projects = { directory = "/srv/git" }
            "#,
        )?;

        assert_eq!(
            *config.settings.projects.directory,
            PathBuf::from("/srv/git")
        );

        Ok(())
    }

    #[test]
    fn test_can_parse_config() -> anyhow::Result<()> {
        let content = r#"
              [settings]
              projects = { directory = "git" }
        
              [settings.cache]
              location = ".cache/gitnow"
              duration = { days = 2 }

              [[providers.github]]  
              current_user = "kjuulh"
              access_token = "some-token"
              users = ["kjuulh"]
              organisations = ["lunarway"]

              [[providers.github]]  
              access_token = { env = "something" }
              users = ["other"]
              organisations = ["org"]

              [[providers.gitea]]  
              url = "https://git.front.kjuulh.io/api/v1"
              current_user = "kjuulh"
              users = ["kjuulh"]
              organisations = ["lunarway"]

              [[providers.gitea]]  
              url = "https://git.front.kjuulh.io/api/v1"
              users = ["other"]
              organisations = ["org"]

              [[providers.gitea]]  
              url = "https://git.front.kjuulh.io/api/v1"
            "#;

        let config = Config::from_string(content)?;

        pretty_assertions::assert_eq!(
            Config {
                providers: Providers {
                    github: vec![
                        GitHub {
                            users: vec![GitHubUser("kjuulh".into())],
                            organisations: vec![GitHubOrganisation("lunarway".into())],
                            url: None,
                            access_token: GitHubAccessToken::Direct("some-token".into()),
                            current_user: Some("kjuulh".into())
                        },
                        GitHub {
                            users: vec![GitHubUser("other".into())],
                            organisations: vec![GitHubOrganisation("org".into())],
                            url: None,
                            access_token: GitHubAccessToken::Env {
                                env: "something".into()
                            },
                            current_user: None
                        }
                    ],
                    gitea: vec![
                        Gitea {
                            url: "https://git.front.kjuulh.io/api/v1".into(),
                            users: vec![GiteaUser("kjuulh".into())],
                            organisations: vec![GiteaOrganisation("lunarway".into())],
                            access_token: None,
                            current_user: Some("kjuulh".into())
                        },
                        Gitea {
                            url: "https://git.front.kjuulh.io/api/v1".into(),
                            users: vec![GiteaUser("other".into())],
                            organisations: vec![GiteaOrganisation("org".into())],
                            access_token: None,
                            current_user: None
                        },
                        Gitea {
                            url: "https://git.front.kjuulh.io/api/v1".into(),
                            users: vec![],
                            organisations: vec![],
                            access_token: None,
                            current_user: None
                        },
                    ]
                },
                settings: Settings {
                    cache: Cache {
                        location: PathBuf::from(".cache/gitnow").into(),
                        duration: CacheDuration::Precise {
                            days: 2,
                            hours: 0,
                            minutes: 0
                        }
                    },
                    projects: Projects {
                        directory: PathBuf::from("git").into()
                    },
                    post_update_command: None,
                    post_update_command_by_vcs: PostUpdateCommandsByVcs::default(),
                    post_clone_command: None,
                    clone_command: None,
                    worktree: None,
                    project: None,
                }
            },
            config
        );

        Ok(())
    }

    #[test]
    fn test_can_parse_empty_config() -> anyhow::Result<()> {
        let content = r#"
                # empty file
            "#;

        let config = Config::from_string(content)?;

        assert_eq!(
            Config {
                providers: Providers {
                    github: vec![],
                    gitea: vec![]
                },
                settings: Settings {
                    cache: Cache::default(),
                    projects: Projects::default(),
                    post_update_command: None,
                    post_update_command_by_vcs: PostUpdateCommandsByVcs::default(),
                    post_clone_command: None,
                    clone_command: None,
                    worktree: None,
                    project: None,
                }
            },
            config
        );

        Ok(())
    }

    #[test]
    fn test_can_parse_config_with_clone_command() -> anyhow::Result<()> {
        let content = r#"
              [settings]
              projects = { directory = "git" }
              clone_command = "jj git clone {{ ssh_url }} {{ path }}"
            "#;

        let config = Config::from_string(content)?;

        assert_eq!(
            config.settings.clone_command,
            Some("jj git clone {{ ssh_url }} {{ path }}".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_can_parse_config_with_worktree() -> anyhow::Result<()> {
        let content = r#"
              [settings]
              projects = { directory = "git" }

              [settings.worktree]
              clone_command = "jj git clone {{ ssh_url }} {{ bare_path }}"
              add_command = "jj workspace add --name {{ branch }} {{ worktree_path }}"
              list_branches_command = "jj -R {{ bare_path }} branch list"
            "#;

        let config = Config::from_string(content)?;

        assert_eq!(
            config.settings.worktree,
            Some(WorktreeSettings {
                clone_command: Some("jj git clone {{ ssh_url }} {{ bare_path }}".to_string()),
                add_command: Some(
                    "jj workspace add --name {{ branch }} {{ worktree_path }}".to_string()
                ),
                list_branches_command: Some("jj -R {{ bare_path }} branch list".to_string()),
            })
        );

        Ok(())
    }

    #[test]
    fn test_can_parse_automatic_project_cleanup() -> anyhow::Result<()> {
        let content = r#"
              [settings.project]
              directory = "~/.gitnow/projects"
              templates_directory = "~/.gitnow/templates"
              auto_delete_older_than_days = 30
            "#;

        let config = Config::from_string(content)?;
        let project = config.settings.project.unwrap();

        assert_eq!(project.auto_delete_older_than_days, Some(30));

        Ok(())
    }
}
