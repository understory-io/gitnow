use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use commands::{
    clone::CloneCommand, list::ListCommand, project::ProjectCommand, root::RootCommand,
    shell::Shell, skill::SkillCommand, update::Update, worktree::WorktreeCommand,
};
use config::Config;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

mod app;
mod cache;
mod cache_codec;
pub mod chooser;
mod commands;
mod components;
mod config;
mod custom_command;
mod fuzzy_matcher;
mod git_clone;
mod git_provider;
mod interactive;
mod project_metadata;
mod projects_list;
mod shell;
mod subprocess;
mod template_command;
mod terminal;
mod worktree;
mod zero_config;

#[derive(Parser)]
#[command(
    author,
    version,
    about,
    long_about = Some("Navigate git projects at the speed of thought"),
    after_help = "TIP: LLM/AI agents should run `gitnow skill` for a complete machine-readable reference."
)]
struct Command {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to config file (default: ~/.config/gitnow/gitnow.toml, or $GITNOW_CONFIG)
    #[arg(long = "config", short = 'c', global = true)]
    config: Option<PathBuf>,

    /// Repository query terms. All terms must match.
    #[arg(value_name = "QUERY", num_args = 0..)]
    search: Vec<String>,

    /// Always show the interactive picker, pre-filtered by the query.
    #[arg(long = "interactive", short = 'i', default_value = "false")]
    interactive: bool,

    #[arg(long = "no-cache", default_value = "false")]
    no_cache: bool,

    #[arg(long = "no-clone", default_value = "false")]
    no_clone: bool,

    #[arg(long = "no-shell", default_value = "false")]
    no_shell: bool,

    /// Path to a chooser file; if set, the selected directory path is written
    /// to this file instead of spawning a shell or printing to stdout.
    /// Can also be set via the GITNOW_CHOOSER_FILE environment variable.
    #[arg(long = "chooser-file", global = true, env = "GITNOW_CHOOSER_FILE")]
    chooser_file: Option<PathBuf>,

    #[arg(long = "force-refresh", default_value = "false")]
    force_refresh: bool,

    #[arg(long = "force-cache-update", default_value = "false")]
    force_cache_update: bool,
}

#[derive(Subcommand)]
enum Commands {
    Init(Shell),
    Update(Update),
    Clone(CloneCommand),
    /// List known repositories without opening one
    List(ListCommand),
    Worktree(WorktreeCommand),
    /// Manage scratch-pad projects with multiple repositories
    Project(ProjectCommand),
    /// Print an LLM-readable reference of all gitnow capabilities
    Skill(SkillCommand),
}

const DEFAULT_CONFIG_PATH: &str = ".config/gitnow/gitnow.toml";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::ERROR.into())
                .from_env_lossy(),
        )
        .init();

    let cli = Command::parse();
    tracing::debug!("Starting cli");

    let config_path = if let Some(path) = &cli.config {
        path.clone()
    } else {
        let home =
            std::env::var("HOME").context("HOME was not found, are you using a proper shell?")?;
        let default_config_path = PathBuf::from(home).join(DEFAULT_CONFIG_PATH);
        std::env::var("GITNOW_CONFIG")
            .map(PathBuf::from)
            .unwrap_or(default_config_path)
    };

    let config = Config::from_file(&config_path).await?;

    let app = app::App::new_static(config).await?;

    // When a chooser file is provided, it implies --no-shell behaviour:
    // the selected path is written to the file and no interactive shell is
    // spawned.  The calling shell wrapper is responsible for reading the
    // file and changing directory.
    let chooser = cli
        .chooser_file
        .map(chooser::Chooser::new)
        .unwrap_or_default();
    let no_shell = cli.no_shell || chooser.is_active();
    let search = (!cli.search.is_empty()).then(|| cli.search.join(" "));

    match cli.command {
        Some(cmd) => match cmd {
            Commands::Init(mut shell) => {
                shell.execute().await?;
            }
            Commands::Update(mut update) => {
                update.execute(app).await?;
            }
            Commands::Clone(mut clone) => {
                clone.execute(app).await?;
            }
            Commands::List(list) => {
                list.execute(app).await?;
            }
            Commands::Worktree(mut wt) => {
                wt.execute(app, &chooser).await?;
            }
            Commands::Project(mut project) => {
                project.execute(app, &chooser).await?;
            }
            Commands::Skill(skill) => {
                skill.execute().await?;
            }
        },
        None => {
            RootCommand::new(app)
                .execute(
                    search.as_deref(),
                    cli.interactive,
                    commands::root::RootOptions {
                        cache: !cli.no_cache,
                        clone: !cli.no_clone,
                        shell: !no_shell,
                        force_refresh: cli.force_refresh,
                        force_cache_update: cli.force_cache_update,
                    },
                    &chooser,
                )
                .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Command;

    #[test]
    fn accepts_multiple_query_terms() {
        let command = Command::try_parse_from(["gitnow", "mire", "api"]).unwrap();

        assert_eq!(command.search, ["mire", "api"]);
        assert!(!command.interactive);
    }

    #[test]
    fn list_does_not_swallow_the_root_query() {
        // `list` is a subcommand, so its terms must land on it and not on the root
        // command's free-form query, which would silently open a picker instead.
        let command = Command::try_parse_from(["gitnow", "list", "--json", "mire", "api"]).unwrap();

        assert!(command.search.is_empty());
        assert!(matches!(command.command, Some(super::Commands::List(_))));
    }

    #[test]
    fn accepts_an_initial_interactive_query() {
        let command = Command::try_parse_from(["gitnow", "--interactive", "mire", "api"]).unwrap();

        assert_eq!(command.search, ["mire", "api"]);
        assert!(command.interactive);
    }
}
