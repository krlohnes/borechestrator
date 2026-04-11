use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;

/// Specification for a job to be executed.
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub hat_id: String,
    pub run_id: String,
    pub command: String,
    pub env: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
}

/// Handle to a running or completed job.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobHandle {
    pub id: String,
}

/// Status of a job.
#[derive(Debug)]
pub enum JobStatus {
    Succeeded { stdout: String },
    Failed { reason: String, stdout: String },
}

/// Abstraction over agent execution (local process, Docker, K8s Job).
#[async_trait]
pub trait Runtime: Send + Sync {
    /// Start execution and return a handle.
    async fn create_job(&self, spec: JobSpec) -> anyhow::Result<JobHandle>;

    /// Wait for the job to complete and return its status.
    async fn wait_job(&self, handle: &JobHandle) -> anyhow::Result<JobStatus>;

    /// Cancel a running job.
    async fn cancel_job(&self, handle: &JobHandle) -> anyhow::Result<()>;
}
