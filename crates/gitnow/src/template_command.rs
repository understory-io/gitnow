use std::collections::HashMap;

use anyhow::Context;

pub const DEFAULT_CLONE_COMMAND: &str = "git clone {{ ssh_url }} {{ path }}";
pub const DEFAULT_WORKTREE_CLONE_COMMAND: &str =
    "git clone --bare {{ ssh_url }} {{ bare_path }}";
pub const DEFAULT_WORKTREE_ADD_COMMAND: &str =
    "git -C {{ bare_path }} worktree add {{ worktree_path }} {{ branch }}";
pub const DEFAULT_LIST_BRANCHES_COMMAND: &str =
    "git -C {{ bare_path }} branch --format=%(refname:short)";

pub async fn render_and_execute(
    template: &str,
    context: HashMap<&str, &str>,
) -> anyhow::Result<std::process::Output> {
    let (program, args) = render_command_parts(template, &context)?;

    tracing::debug!("executing: {} {}", program, args.join(" "));

    let output = tokio::process::Command::new(&program)
        .args(&args)
        .output()
        .await
        .with_context(|| format!("failed to execute: {} {}", program, args.join(" ")))?;

    Ok(output)
}

fn render_command_parts(
    template: &str,
    context: &HashMap<&str, &str>,
) -> anyhow::Result<(String, Vec<String>)> {
    let env = minijinja::Environment::new();
    let rendered = env
        .render_str(template, context)
        .context("failed to render command template")?;

    let parts =
        shell_words::split(&rendered).context("failed to parse rendered command as shell words")?;

    let (program, args) = parts
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("command template rendered to empty string"))?;

    Ok((program.clone(), args.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_clone_command() {
        let context = HashMap::from([
            ("ssh_url", "ssh://git@github.com/owner/repo.git"),
            ("path", "/home/user/git/github.com/owner/repo"),
        ]);

        let (program, args) = render_command_parts(DEFAULT_CLONE_COMMAND, &context).unwrap();
        assert_eq!(program, "git");
        assert_eq!(
            args,
            vec![
                "clone",
                "ssh://git@github.com/owner/repo.git",
                "/home/user/git/github.com/owner/repo"
            ]
        );
    }

    #[test]
    fn test_render_jj_clone_command() {
        let template = "jj git clone {{ ssh_url }} {{ path }}";
        let context = HashMap::from([
            ("ssh_url", "ssh://git@github.com/owner/repo.git"),
            ("path", "/home/user/git/github.com/owner/repo"),
        ]);

        let (program, args) = render_command_parts(template, &context).unwrap();
        assert_eq!(program, "jj");
        assert_eq!(
            args,
            vec![
                "git",
                "clone",
                "ssh://git@github.com/owner/repo.git",
                "/home/user/git/github.com/owner/repo"
            ]
        );
    }

    #[test]
    fn custom_clone_command_can_use_https_clone_url() {
        let template = "git clone {{ clone_url }} {{ path }}";
        let context = HashMap::from([
            ("clone_url", "https://github.com/owner/repo.git"),
            ("path", "/home/user/git/github.com/owner/repo"),
        ]);

        let (program, args) = render_command_parts(template, &context).unwrap();

        assert_eq!(
            (program, args),
            (
                "git".to_string(),
                vec![
                    "clone".to_string(),
                    "https://github.com/owner/repo.git".to_string(),
                    "/home/user/git/github.com/owner/repo".to_string(),
                ]
            )
        );
    }

    #[test]
    fn test_render_worktree_clone_command() {
        let context = HashMap::from([
            ("ssh_url", "ssh://git@github.com/owner/repo.git"),
            (
                "bare_path",
                "/home/user/git/github.com/owner/repo/.bare",
            ),
        ]);

        let (program, args) =
            render_command_parts(DEFAULT_WORKTREE_CLONE_COMMAND, &context).unwrap();
        assert_eq!(program, "git");
        assert_eq!(
            args,
            vec![
                "clone",
                "--bare",
                "ssh://git@github.com/owner/repo.git",
                "/home/user/git/github.com/owner/repo/.bare"
            ]
        );
    }

    #[test]
    fn test_render_worktree_add_command() {
        let context = HashMap::from([
            (
                "bare_path",
                "/home/user/git/github.com/owner/repo/.bare",
            ),
            (
                "worktree_path",
                "/home/user/git/github.com/owner/repo/feature-x",
            ),
            ("branch", "feature/x"),
        ]);

        let (program, args) =
            render_command_parts(DEFAULT_WORKTREE_ADD_COMMAND, &context).unwrap();
        assert_eq!(program, "git");
        assert_eq!(
            args,
            vec![
                "-C",
                "/home/user/git/github.com/owner/repo/.bare",
                "worktree",
                "add",
                "/home/user/git/github.com/owner/repo/feature-x",
                "feature/x"
            ]
        );
    }
}
