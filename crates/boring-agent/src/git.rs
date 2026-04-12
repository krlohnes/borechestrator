use anyhow::Context;
use std::path::Path;
use tokio::process::Command;
use tracing::info;

/// Clone a git repo and set up the working branch.
pub async fn clone_and_checkout(
    repo_url: &str,
    base_branch: &str,
    work_branch: &str,
    target_dir: &Path,
    token: Option<&str>,
) -> anyhow::Result<()> {
    // If a token is provided, inject it into the URL for HTTPS auth
    let clone_url = if let Some(token) = token {
        inject_token(repo_url, token)
    } else {
        repo_url.to_string()
    };

    // Set git user config for commits inside containers
    let _ = Command::new("git")
        .args(["config", "--global", "user.email", "boring@borechestrator.dev"])
        .status()
        .await;
    let _ = Command::new("git")
        .args(["config", "--global", "user.name", "borechestrator"])
        .status()
        .await;

    // Try to clone the work branch directly (it may already exist from a previous hat).
    // If it doesn't exist, clone the base branch and create the work branch.
    info!(repo = %repo_url, work_branch = %work_branch, "cloning repository");

    let status = Command::new("git")
        .args(["clone", "--branch", work_branch, "--single-branch", &clone_url])
        .arg(target_dir)
        .status()
        .await;

    if status.is_ok() && status.unwrap().success() {
        info!(branch = %work_branch, "cloned existing work branch");
    } else {
        // Work branch doesn't exist yet — clone base and create it
        // Clean up failed clone attempt
        let _ = tokio::fs::remove_dir_all(target_dir).await;

        info!(branch = %base_branch, "work branch not found, cloning base");
        let status = Command::new("git")
            .args(["clone", "--branch", base_branch, "--single-branch", &clone_url])
            .arg(target_dir)
            .status()
            .await
            .context("failed to clone base branch")?;

        if !status.success() {
            anyhow::bail!("git clone failed with exit code {:?}", status.code());
        }

        info!(branch = %work_branch, "creating work branch");
        let status = Command::new("git")
            .args(["checkout", "-b", work_branch])
            .current_dir(target_dir)
            .status()
            .await
            .context("failed to create work branch")?;

        if !status.success() {
            anyhow::bail!("git checkout -b failed with exit code {:?}", status.code());
        }
    }

    // Install pre-push hook that rebases before push.
    // If rebase has conflicts, the error message tells the agent to fix them.
    let hooks_dir = target_dir.join(".git/hooks");
    let hook = r#"#!/bin/sh
BRANCH=$(git branch --show-current)
# Only rebase if the branch already exists on the remote
if git ls-remote --exit-code origin "$BRANCH" >/dev/null 2>&1; then
    echo "pre-push: rebasing onto origin/$BRANCH"
    git fetch origin "$BRANCH" 2>/dev/null
    if ! git rebase "origin/$BRANCH"; then
        echo ""
        echo "ERROR: Rebase conflicts detected."
        echo "Fix the conflicts in the files listed above, then run:"
        echo "  git add <fixed files>"
        echo "  git rebase --continue"
        echo "  git push origin $BRANCH"
        exit 1
    fi
else
    echo "pre-push: new branch, no rebase needed"
fi
"#;
    tokio::fs::write(hooks_dir.join("pre-push"), &hook).await.ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            hooks_dir.join("pre-push"),
            std::fs::Permissions::from_mode(0o755),
        ).ok();
    }
    info!("installed pre-push hook");

    Ok(())
}

/// Push the work branch to the remote.
pub async fn push(target_dir: &Path, branch: &str) -> anyhow::Result<bool> {
    // Check if there are any commits beyond the base
    let output = Command::new("git")
        .args(["log", "--oneline", &format!("origin/HEAD..{}", branch)])
        .current_dir(target_dir)
        .output()
        .await
        .context("failed to check for new commits")?;

    let log_output = String::from_utf8_lossy(&output.stdout);
    if log_output.trim().is_empty() {
        info!("no new commits to push");
        return Ok(false);
    }

    info!(branch = %branch, "pushing work branch");

    let status = Command::new("git")
        .args(["push", "-u", "origin", branch])
        .current_dir(target_dir)
        .status()
        .await
        .context("failed to push")?;

    if !status.success() {
        anyhow::bail!("git push failed with exit code {:?}", status.code());
    }

    Ok(true)
}

/// Inject a token into an HTTPS git URL for authentication.
/// https://github.com/org/repo.git -> https://x-access-token:TOKEN@github.com/org/repo.git
fn inject_token(url: &str, token: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://") {
        format!("https://x-access-token:{}@{}", token, rest)
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_token_https() {
        let url = inject_token("https://github.com/org/repo.git", "ghp_abc123");
        assert_eq!(url, "https://x-access-token:ghp_abc123@github.com/org/repo.git");
    }

    #[test]
    fn test_inject_token_non_https_passthrough() {
        let url = inject_token("git@github.com:org/repo.git", "token");
        assert_eq!(url, "git@github.com:org/repo.git");
    }
}
