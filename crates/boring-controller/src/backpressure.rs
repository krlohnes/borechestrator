use boring_proto::config::Gate;
use tokio::process::Command;
use tracing::{info, warn};

/// Run a set of gates. Returns Ok(()) if all pass, Err with the failing gate's name/message if any fail.
pub async fn run_gates(gates: &[Gate]) -> Result<(), GateFailure> {
    for gate in gates {
        info!(gate = %gate.name, "running backpressure gate");

        let output = Command::new("sh")
            .arg("-c")
            .arg(&gate.command)
            .output()
            .await
            .map_err(|e| GateFailure {
                gate_name: gate.name.clone(),
                reason: format!("failed to execute: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let reason = gate.on_fail.clone().unwrap_or_else(|| {
                if stderr.is_empty() {
                    format!(
                        "gate '{}' failed with exit code {:?}",
                        gate.name,
                        output.status.code()
                    )
                } else {
                    stderr
                }
            });

            warn!(gate = %gate.name, reason = %reason, "backpressure gate failed");
            return Err(GateFailure {
                gate_name: gate.name.clone(),
                reason,
            });
        }

        info!(gate = %gate.name, "gate passed");
    }

    Ok(())
}

#[derive(Debug)]
pub struct GateFailure {
    pub gate_name: String,
    pub reason: String,
}

impl std::fmt::Display for GateFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "gate '{}' failed: {}", self.gate_name, self.reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(name: &str, command: &str) -> Gate {
        Gate {
            name: name.to_string(),
            command: command.to_string(),
            on_fail: None,
        }
    }

    #[tokio::test]
    async fn test_passing_gate() {
        let gates = vec![gate("fmt", "true")];
        assert!(run_gates(&gates).await.is_ok());
    }

    #[tokio::test]
    async fn test_failing_gate() {
        let gates = vec![gate("fmt", "false")];
        let result = run_gates(&gates).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().gate_name, "fmt");
    }

    #[tokio::test]
    async fn test_multiple_gates_all_pass() {
        let gates = vec![
            gate("step1", "true"),
            gate("step2", "true"),
            gate("step3", "true"),
        ];
        assert!(run_gates(&gates).await.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_gates_second_fails() {
        let gates = vec![
            gate("step1", "true"),
            gate("step2", "false"),
            gate("step3", "true"),
        ];
        let result = run_gates(&gates).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().gate_name, "step2");
    }

    #[tokio::test]
    async fn test_empty_gates() {
        let gates: Vec<Gate> = vec![];
        assert!(run_gates(&gates).await.is_ok());
    }

    #[tokio::test]
    async fn test_custom_on_fail_message() {
        let gates = vec![Gate {
            name: "lint".to_string(),
            command: "false".to_string(),
            on_fail: Some("Code formatting failed. Run cargo fmt.".to_string()),
        }];
        let result = run_gates(&gates).await;
        let err = result.unwrap_err();
        assert!(err.reason.contains("cargo fmt"));
    }

    #[tokio::test]
    async fn test_gate_with_real_command() {
        let gates = vec![gate("echo-check", "echo 'hello' | grep hello")];
        assert!(run_gates(&gates).await.is_ok());
    }
}
