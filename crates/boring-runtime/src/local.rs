use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::traits::{JobHandle, JobSpec, JobStatus, Runtime};

/// Local process runtime. Spawns shell commands as child processes.
pub struct LocalRuntime {
    children: Arc<Mutex<HashMap<String, tokio::process::Child>>>,
    next_id: Arc<Mutex<u64>>,
}

impl LocalRuntime {
    pub fn new() -> Self {
        Self {
            children: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl Runtime for LocalRuntime {
    async fn create_job(&self, spec: JobSpec) -> anyhow::Result<JobHandle> {
        let mut id_lock = self.next_id.lock().await;
        let id = format!("{}-{}-{}", spec.run_id, spec.hat_id, *id_lock);
        *id_lock += 1;
        drop(id_lock);

        let mut cmd = Command::new("bash");
        // Tee stdout to stderr so the user sees output live while we capture it
        // pipefail ensures we get the command's exit code, not tee's
        let teed = format!("set -o pipefail; ({}) 2>&1 | tee /dev/stderr", spec.command);
        cmd.arg("-c").arg(&teed);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::inherit());

        for (key, value) in &spec.env {
            cmd.env(key, value);
        }

        if let Some(ref dir) = spec.working_dir {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn()?;
        self.children.lock().await.insert(id.clone(), child);

        Ok(JobHandle { id })
    }

    async fn wait_job(&self, handle: &JobHandle) -> anyhow::Result<JobStatus> {
        let child = self.children.lock().await.remove(&handle.id);
        let Some(child) = child else {
            return Ok(JobStatus::Failed {
                reason: "job not found".to_string(),
                stdout: String::new(),
            });
        };

        let output = child.wait_with_output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        if output.status.success() {
            Ok(JobStatus::Succeeded { stdout })
        } else {
            // stderr is inherited (displayed live), so use exit code for the reason
            let reason = format!("exit code: {}", output.status.code().unwrap_or(-1));
            Ok(JobStatus::Failed { reason, stdout })
        }
    }

    async fn cancel_job(&self, handle: &JobHandle) -> anyhow::Result<()> {
        if let Some(mut child) = self.children.lock().await.remove(&handle.id) {
            child.kill().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn simple_spec(command: &str) -> JobSpec {
        JobSpec {
            hat_id: "test-hat".to_string(),
            run_id: "run-test".to_string(),
            command: command.to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_successful_job() {
        let runtime = LocalRuntime::new();
        let spec = simple_spec("echo 'hello from hat'");
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                assert!(stdout.contains("hello from hat"));
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_failed_job() {
        let runtime = LocalRuntime::new();
        let spec = simple_spec("exit 1");
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Failed { reason, .. } => {
                assert!(!reason.is_empty());
            }
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_job_receives_env_vars() {
        let runtime = LocalRuntime::new();
        let mut spec = simple_spec("echo $BORING_HAT_ID");
        spec.env
            .insert("BORING_HAT_ID".to_string(), "planner".to_string());
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                assert!(stdout.contains("planner"));
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_job_with_working_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let runtime = LocalRuntime::new();
        let mut spec = simple_spec("pwd");
        spec.working_dir = Some(dir.path().to_path_buf());
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                let expected = dir.path().canonicalize().unwrap();
                let actual = PathBuf::from(stdout.trim()).canonicalize().unwrap();
                assert_eq!(actual, expected);
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_cancel_job() {
        let runtime = LocalRuntime::new();
        let spec = simple_spec("sleep 60");
        let handle = runtime.create_job(spec).await.unwrap();

        runtime.cancel_job(&handle).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Failed { .. } => {}
            other => panic!("expected Failed after cancel, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_multiline_stdout() {
        let runtime = LocalRuntime::new();
        let spec = simple_spec("echo 'line1'; echo 'line2'; echo 'line3'");
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                let lines: Vec<&str> = stdout.trim().lines().collect();
                assert_eq!(lines.len(), 3);
                assert_eq!(lines[0], "line1");
                assert_eq!(lines[1], "line2");
                assert_eq!(lines[2], "line3");
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }
}
