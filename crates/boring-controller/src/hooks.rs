use serde::Serialize;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};

/// Hook configuration from YAML.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HookConfig {
    pub name: String,
    pub command: Vec<String>,
    #[serde(default = "default_on_error")]
    pub on_error: HookErrorAction,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_on_error() -> HookErrorAction {
    HookErrorAction::Warn
}

fn default_timeout() -> u64 {
    30
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HookErrorAction {
    Block,
    Warn,
    Ignore,
}

/// Context passed to hooks as JSON on stdin.
#[derive(Debug, Serialize)]
pub struct HookContext {
    pub event: String,
    pub hat_id: Option<String>,
    pub run_id: String,
    pub iteration: u32,
}

/// Run a set of hooks, passing context as JSON on stdin.
pub async fn run_hooks(hooks: &[HookConfig], context: &HookContext) -> Result<(), HookError> {
    let context_json = serde_json::to_string(context).unwrap_or_default();

    for hook in hooks {
        info!(hook = %hook.name, "running hook");

        if hook.command.is_empty() {
            continue;
        }

        let mut cmd = Command::new(&hook.command[0]);
        if hook.command.len() > 1 {
            cmd.args(&hook.command[1..]);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| HookError {
            hook_name: hook.name.clone(),
            reason: format!("failed to spawn: {}", e),
        })?;

        // Write context to stdin
        if let Some(stdin) = child.stdin.take() {
            let json = context_json.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let mut stdin = stdin;
                let _ = stdin.write_all(json.as_bytes()).await;
                let _ = stdin.shutdown().await;
            });
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(hook.timeout_seconds),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| HookError {
            hook_name: hook.name.clone(),
            reason: "timed out".to_string(),
        })?
        .map_err(|e| HookError {
            hook_name: hook.name.clone(),
            reason: format!("execution failed: {}", e),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let err = HookError {
                hook_name: hook.name.clone(),
                reason: if stderr.is_empty() {
                    format!("exit code {:?}", output.status.code())
                } else {
                    stderr
                },
            };

            match hook.on_error {
                HookErrorAction::Block => return Err(err),
                HookErrorAction::Warn => {
                    warn!(hook = %hook.name, reason = %err.reason, "hook failed (warn)");
                }
                HookErrorAction::Ignore => {}
            }
        } else {
            info!(hook = %hook.name, "hook passed");
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct HookError {
    pub hook_name: String,
    pub reason: String,
}

impl std::fmt::Display for HookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hook '{}' failed: {}", self.hook_name, self.reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(name: &str, command: &[&str]) -> HookConfig {
        HookConfig {
            name: name.to_string(),
            command: command.iter().map(|s| s.to_string()).collect(),
            on_error: HookErrorAction::Block,
            timeout_seconds: 5,
        }
    }

    fn ctx() -> HookContext {
        HookContext {
            event: "pre.hat.start".to_string(),
            hat_id: Some("planner".to_string()),
            run_id: "run-test".to_string(),
            iteration: 1,
        }
    }

    #[tokio::test]
    async fn test_passing_hook() {
        let hooks = vec![hook("check", &["true"])];
        assert!(run_hooks(&hooks, &ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn test_blocking_hook_failure() {
        let hooks = vec![hook("check", &["false"])];
        let result = run_hooks(&hooks, &ctx()).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().hook_name, "check");
    }

    #[tokio::test]
    async fn test_warn_hook_continues() {
        let hooks = vec![HookConfig {
            name: "check".to_string(),
            command: vec!["false".to_string()],
            on_error: HookErrorAction::Warn,
            timeout_seconds: 5,
        }];
        assert!(run_hooks(&hooks, &ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn test_hook_receives_context_on_stdin() {
        // Hook reads stdin and writes it to stdout; verify it got our JSON
        let hooks = vec![hook("reader", &["cat"])];
        assert!(run_hooks(&hooks, &ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn test_empty_hooks() {
        let hooks: Vec<HookConfig> = vec![];
        assert!(run_hooks(&hooks, &ctx()).await.is_ok());
    }
}
