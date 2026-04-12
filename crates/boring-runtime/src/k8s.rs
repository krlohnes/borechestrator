use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{
    Container, EnvVar, PodSpec, PodTemplateSpec, Volume, VolumeMount,
    SecretVolumeSource,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, PostParams, LogParams};
use kube::{Client, ResourceExt};
use tokio::sync::Mutex;

use crate::traits::{JobHandle, JobSpec, JobStatus, Runtime};

/// Kubernetes Job runtime. Creates K8s Jobs for hat activations.
pub struct K8sRuntime {
    client: Client,
    namespace: String,
    default_image: String,
    active_jobs: Arc<Mutex<HashMap<String, String>>>, // handle_id -> k8s job name
}

impl K8sRuntime {
    pub async fn new(namespace: &str, default_image: &str) -> anyhow::Result<Self> {
        let client = Client::try_default().await?;
        Ok(Self {
            client,
            namespace: namespace.to_string(),
            default_image: default_image.to_string(),
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn with_client(client: Client, namespace: &str, default_image: &str) -> Self {
        Self {
            client,
            namespace: namespace.to_string(),
            default_image: default_image.to_string(),
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Runtime for K8sRuntime {
    async fn create_job(&self, spec: JobSpec) -> anyhow::Result<JobHandle> {
        let jobs: Api<Job> = Api::namespaced(self.client.clone(), &self.namespace);

        // Sanitize name for K8s (lowercase, alphanumeric + hyphens, max 63 chars)
        let job_name = format!(
            "boring-{}-{}",
            spec.hat_id,
            &uuid::Uuid::new_v4().to_string()[..8]
        )
        .to_lowercase()
        .replace('_', "-");
        let job_name = if job_name.len() > 63 {
            job_name[..63].to_string()
        } else {
            job_name
        };

        let image = spec
            .env
            .get("BORING_IMAGE")
            .cloned()
            .unwrap_or_else(|| self.default_image.clone());

        let env_vars: Vec<EnvVar> = spec
            .env
            .iter()
            .map(|(k, v)| EnvVar {
                name: k.clone(),
                value: Some(v.clone()),
                ..Default::default()
            })
            .collect();

        let k8s_job = Job {
            metadata: ObjectMeta {
                name: Some(job_name.clone()),
                namespace: Some(self.namespace.clone()),
                labels: Some(
                    [
                        ("app.kubernetes.io/managed-by".to_string(), "borechestrator".to_string()),
                        ("borechestrator/run-id".to_string(), spec.run_id.clone()),
                        ("borechestrator/hat-id".to_string(), spec.hat_id.clone()),
                    ]
                    .into(),
                ),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::batch::v1::JobSpec {
                backoff_limit: Some(0),
                active_deadline_seconds: Some(600),
                template: PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: Some(
                            [
                                ("app.kubernetes.io/managed-by".to_string(), "borechestrator".to_string()),
                                ("borechestrator/hat-id".to_string(), spec.hat_id.clone()),
                            ]
                            .into(),
                        ),
                        ..Default::default()
                    }),
                    spec: Some({
                        // Build volume mounts and volumes from secret_mounts
                        let mut volume_mounts = Vec::new();
                        let mut volumes = Vec::new();

                        for (i, (secret_name, mount_path)) in spec.secret_mounts.iter().enumerate() {
                            let vol_name = format!("secret-{}", i);
                            let key = std::path::Path::new(mount_path)
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| "secret".to_string());

                            volume_mounts.push(VolumeMount {
                                name: vol_name.clone(),
                                mount_path: mount_path.clone(),
                                sub_path: Some(key.clone()),
                                read_only: Some(true),
                                ..Default::default()
                            });

                            volumes.push(Volume {
                                name: vol_name,
                                secret: Some(SecretVolumeSource {
                                    secret_name: Some(secret_name.clone()),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            });
                        }

                        PodSpec {
                            restart_policy: Some("Never".to_string()),
                            containers: vec![Container {
                                name: "agent".to_string(),
                                image: Some(image),
                                image_pull_policy: Some("IfNotPresent".to_string()),
                                // Don't override command — let the image's ENTRYPOINT
                                // (boring-agent) handle it via BORING_COMMAND env var.
                                // boring-agent reads BORING_COMMAND and runs it.
                                env: Some({
                                    let mut evs = env_vars;
                                    evs.push(EnvVar {
                                        name: "BORING_COMMAND".to_string(),
                                        value: Some(spec.command.clone()),
                                        ..Default::default()
                                    });
                                    evs
                                }),
                                volume_mounts: if volume_mounts.is_empty() { None } else { Some(volume_mounts) },
                                ..Default::default()
                            }],
                            volumes: if volumes.is_empty() { None } else { Some(volumes) },
                            ..Default::default()
                        }
                    }),
                },
                ..Default::default()
            }),
            ..Default::default()
        };

        let created = jobs.create(&PostParams::default(), &k8s_job).await?;
        let created_name = created.name_any();

        let handle_id = format!("{}-{}", spec.run_id, spec.hat_id);
        self.active_jobs
            .lock()
            .await
            .insert(handle_id.clone(), created_name);

        Ok(JobHandle { id: handle_id })
    }

    async fn wait_job(&self, handle: &JobHandle) -> anyhow::Result<JobStatus> {
        let job_name = self
            .active_jobs
            .lock()
            .await
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("job not found: {}", handle.id))?;

        let jobs: Api<Job> = Api::namespaced(self.client.clone(), &self.namespace);

        // Poll for job completion
        loop {
            let job = jobs.get(&job_name).await?;

            if let Some(ref status) = job.status {
                if let Some(succeeded) = status.succeeded {
                    if succeeded > 0 {
                        // Get logs
                        let stdout = self.get_pod_logs(&job_name).await.unwrap_or_default();
                        self.active_jobs.lock().await.remove(&handle.id);
                        return Ok(JobStatus::Succeeded { stdout });
                    }
                }
                if let Some(failed) = status.failed {
                    if failed > 0 {
                        let stdout = self.get_pod_logs(&job_name).await.unwrap_or_default();
                        let reason = status
                            .conditions
                            .as_ref()
                            .and_then(|c| c.last())
                            .and_then(|c| c.message.clone())
                            .unwrap_or_else(|| "job failed".to_string());
                        self.active_jobs.lock().await.remove(&handle.id);
                        return Ok(JobStatus::Failed { reason, stdout });
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    async fn cancel_job(&self, handle: &JobHandle) -> anyhow::Result<()> {
        if let Some(job_name) = self.active_jobs.lock().await.remove(&handle.id) {
            let jobs: Api<Job> = Api::namespaced(self.client.clone(), &self.namespace);
            let dp = kube::api::DeleteParams {
                propagation_policy: Some(kube::api::PropagationPolicy::Background),
                ..Default::default()
            };
            let _ = jobs.delete(&job_name, &dp).await;
        }
        Ok(())
    }
}

impl K8sRuntime {
    async fn get_pod_logs(&self, job_name: &str) -> anyhow::Result<String> {
        let pods: Api<k8s_openapi::api::core::v1::Pod> =
            Api::namespaced(self.client.clone(), &self.namespace);

        let label = format!("batch.kubernetes.io/job-name={}", job_name);
        let pod_list = pods
            .list(&kube::api::ListParams::default().labels(&label))
            .await?;

        if let Some(pod) = pod_list.items.first() {
            let pod_name = pod.name_any();
            let logs = pods
                .logs(&pod_name, &LogParams::default())
                .await
                .unwrap_or_default();
            Ok(logs)
        } else {
            Ok(String::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // K8s runtime tests require a running K8s cluster (e.g., Docker Desktop K8s).
    // They create real Jobs in the "default" namespace.

    #[tokio::test]
    #[ignore] // requires K8s cluster
    async fn test_k8s_successful_job() {
        let runtime = K8sRuntime::new("default", "alpine:latest").await.unwrap();
        let spec = JobSpec {
            hat_id: "test-hat".to_string(),
            run_id: "k8s-test".to_string(),
            command: "echo 'hello from k8s'".to_string(),
            env: HashMap::new(),
            ..Default::default()
        };

        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                assert!(stdout.contains("hello from k8s"));
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_k8s_failed_job() {
        let runtime = K8sRuntime::new("default", "alpine:latest").await.unwrap();
        let spec = JobSpec {
            hat_id: "test-fail".to_string(),
            run_id: "k8s-test".to_string(),
            command: "exit 1".to_string(),
            env: HashMap::new(),
            ..Default::default()
        };

        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Failed { .. } => {}
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_k8s_env_vars() {
        let runtime = K8sRuntime::new("default", "alpine:latest").await.unwrap();
        let mut env = HashMap::new();
        env.insert("BORING_TEST_VAR".to_string(), "k8s_value".to_string());
        let spec = JobSpec {
            hat_id: "test-env".to_string(),
            run_id: "k8s-test".to_string(),
            command: "echo $BORING_TEST_VAR".to_string(),
            env,
            ..Default::default()
        };

        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                assert!(stdout.contains("k8s_value"));
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_k8s_boring_emit_in_stdout() {
        let runtime = K8sRuntime::new("default", "alpine:latest").await.unwrap();
        let spec = JobSpec {
            hat_id: "test-emit".to_string(),
            run_id: "k8s-test".to_string(),
            command: "echo 'BORING_EMIT subtask.ready do the thing' && echo 'LOOP_COMPLETE'"
                .to_string(),
            env: HashMap::new(),
            ..Default::default()
        };

        let handle = runtime.create_job(spec).await.unwrap();
        let status = runtime.wait_job(&handle).await.unwrap();

        match status {
            JobStatus::Succeeded { stdout } => {
                assert!(stdout.contains("BORING_EMIT subtask.ready"));
                assert!(stdout.contains("LOOP_COMPLETE"));
            }
            other => panic!("expected Succeeded, got {:?}", other),
        }
    }
}
