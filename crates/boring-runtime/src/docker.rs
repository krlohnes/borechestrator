use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::traits::{JobHandle, JobSpec, JobStatus, Runtime};

/// Docker runtime. Runs hat activations as `docker run` commands.
pub struct DockerRuntime {
    children: Arc<Mutex<HashMap<String, tokio::process::Child>>>,
    container_names: Arc<Mutex<HashMap<String, String>>>,
    next_id: Arc<Mutex<u64>>,
    network: Option<String>,
}

impl DockerRuntime {
    pub fn new() -> Self {
        Self {
            children: Arc::new(Mutex::new(HashMap::new())),
            container_names: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
            network: None,
        }
    }

    /// Set the Docker network to attach containers to.
    pub fn with_network(mut self, network: &str) -> Self {
        self.network = Some(network.to_string());
        self
    }
}

#[async_trait]
impl Runtime for DockerRuntime {
    async fn create_job(&self, spec: JobSpec) -> anyhow::Result<JobHandle> {
        let mut id_lock = self.next_id.lock().await;
        let handle_id = format!("{}-{}-{}", spec.run_id, spec.hat_id, *id_lock);
        *id_lock += 1;
        drop(id_lock);

        static GLOBAL_COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = GLOBAL_COUNTER.fetch_add(1, Ordering::Relaxed);
        let container_name = format!("boring-{}-{}", handle_id, unique);

        let image = spec
            .env
            .get("BORING_IMAGE")
            .cloned()
            .unwrap_or_else(|| "ubuntu:24.04".to_string());

        let mut cmd = Command::new("docker");
        cmd.arg("run");
        cmd.arg("--rm");
        cmd.arg("--name").arg(&container_name);

        if let Some(ref network) = self.network {
            cmd.arg("--network").arg(network);
        }

        for (key, value) in &spec.env {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        if let Some(ref dir) = spec.working_dir {
            cmd.arg("-v")
                .arg(format!("{}:/workspace", dir.to_string_lossy()));
            cmd.arg("-w").arg("/workspace");
        }

        cmd.arg(&image);
        let teed = format!("set -o pipefail; ({}) 2>&1 | tee /dev/stderr", spec.command);
        cmd.arg("bash").arg("-c").arg(&teed);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::inherit());

        let child = cmd.spawn()?;

        self.children.lock().await.insert(handle_id.clone(), child);
        self.container_names
            .lock()
            .await
            .insert(handle_id.clone(), container_name);

        Ok(JobHandle { id: handle_id })
    }

    async fn wait_job(&self, handle: &JobHandle) -> anyhow::Result<JobStatus> {
        let child = self.children.lock().await.remove(&handle.id);
        self.container_names.lock().await.remove(&handle.id);

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
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let reason = if stderr.is_empty() {
                format!("exit code: {}", output.status.code().unwrap_or(-1))
            } else {
                stderr
            };
            Ok(JobStatus::Failed { reason, stdout })
        }
    }

    async fn cancel_job(&self, handle: &JobHandle) -> anyhow::Result<()> {
        // Kill the child process (docker run)
        if let Some(mut child) = self.children.lock().await.remove(&handle.id) {
            child.kill().await?;
        }
        // Also try to kill the container directly in case docker run is stuck
        if let Some(container_name) = self.container_names.lock().await.remove(&handle.id) {
            let _ = Command::new("docker")
                .args(["kill", &container_name])
                .output()
                .await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_spec(command: &str) -> JobSpec {
        let mut env = HashMap::new();
        env.insert("BORING_IMAGE".to_string(), "alpine:latest".to_string());
        JobSpec {
            hat_id: "test-hat".to_string(),
            run_id: "run-test".to_string(),
            command: command.to_string(),
            env,
            ..Default::default()
        }
    }

    #[tokio::test]
    #[ignore] // requires Docker
    async fn test_docker_successful_job() {
        let runtime = DockerRuntime::new();
        let spec = simple_spec("echo 'hello from docker'");
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                assert!(stdout.contains("hello from docker"));
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_docker_failed_job() {
        let runtime = DockerRuntime::new();
        let spec = simple_spec("exit 1");
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Failed { .. } => {}
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_docker_env_vars() {
        let runtime = DockerRuntime::new();
        let mut spec = simple_spec("echo $MY_VAR");
        spec.env
            .insert("MY_VAR".to_string(), "hello_env".to_string());
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                assert!(stdout.contains("hello_env"));
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_docker_multiline_stdout() {
        let runtime = DockerRuntime::new();
        let spec = simple_spec("echo 'line1'; echo 'line2'; echo 'BORING_EMIT work.done finished'");
        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                assert!(stdout.contains("line1"));
                assert!(stdout.contains("BORING_EMIT work.done finished"));
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }
}
