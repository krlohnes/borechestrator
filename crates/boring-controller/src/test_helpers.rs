use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;

use boring_broker::traits::{Broker, Subscription};
use boring_proto::event::Event;
use boring_runtime::{JobHandle, JobSpec, JobStatus, Runtime};
use boring_store::Store;

// ---- FakeBroker ----

pub struct FakeBroker {
    published: Arc<Mutex<Vec<Event>>>,
    subscribers: Arc<Mutex<Vec<tokio::sync::mpsc::Sender<Event>>>>,
}

impl FakeBroker {
    pub fn new() -> Self {
        Self {
            published: Arc::new(Mutex::new(Vec::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn published_events(&self) -> Arc<Mutex<Vec<Event>>> {
        self.published.clone()
    }
}

#[async_trait]
impl Broker for FakeBroker {
    async fn publish(&self, _run_id: &str, event: &Event) -> anyhow::Result<()> {
        self.published.lock().await.push(event.clone());
        let subs = self.subscribers.lock().await;
        for tx in subs.iter() {
            let _ = tx.send(event.clone()).await;
        }
        Ok(())
    }

    async fn subscribe(&self, _run_id: &str, _pattern: &str) -> anyhow::Result<Subscription> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        self.subscribers.lock().await.push(tx);
        Ok(Subscription::new(rx))
    }

    async fn subscribe_all(&self, _run_id: &str) -> anyhow::Result<Subscription> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        self.subscribers.lock().await.push(tx);
        Ok(Subscription::new(rx))
    }
}

// ---- FakeStore ----

pub struct FakeStore {
    data: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl FakeStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Store for FakeStore {
    async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.data.lock().await.get(key).cloned())
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> anyhow::Result<()> {
        self.data.lock().await.insert(key.to_string(), value);
        Ok(())
    }

    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let data = self.data.lock().await;
        let mut keys: Vec<String> = data
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        keys.sort();
        Ok(keys)
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.data.lock().await.remove(key);
        Ok(())
    }
}

// ---- FakeRuntime ----

#[derive(Clone)]
pub enum JobResponse {
    SucceedWithEvents(Vec<(String, String)>),
    Fail(String),
}

impl JobResponse {
    pub fn succeed_with_events(events: Vec<(&str, &str)>) -> Self {
        Self::SucceedWithEvents(
            events
                .into_iter()
                .map(|(t, p)| (t.to_string(), p.to_string()))
                .collect(),
        )
    }

    pub fn fail(reason: &str) -> Self {
        Self::Fail(reason.to_string())
    }
}

pub struct FakeRuntime {
    responses: Arc<std::sync::Mutex<HashMap<String, JobResponse>>>,
    created_jobs: Arc<Mutex<Vec<JobSpec>>>,
    pub pending_events: Arc<Mutex<HashMap<String, Vec<Event>>>>,
}

impl FakeRuntime {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(std::sync::Mutex::new(HashMap::new())),
            created_jobs: Arc::new(Mutex::new(Vec::new())),
            pending_events: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn created_jobs(&self) -> Arc<Mutex<Vec<JobSpec>>> {
        self.created_jobs.clone()
    }

    fn get_response(&self, hat_id: &str) -> JobResponse {
        let lock = self.responses.lock().unwrap();
        if let Some(resp) = lock.get(hat_id) {
            return resp.clone();
        }
        JobResponse::SucceedWithEvents(Vec::new())
    }

    pub fn set_response(&self, hat_id: &str, response: JobResponse) {
        self.responses
            .lock()
            .unwrap()
            .insert(hat_id.to_string(), response);
    }
}

#[async_trait]
impl Runtime for FakeRuntime {
    async fn create_job(&self, spec: JobSpec) -> anyhow::Result<JobHandle> {
        let hat_id = spec.hat_id.clone();
        let run_id = spec.run_id.clone();
        let handle_id = format!("{}-{}", run_id, hat_id);

        self.created_jobs.lock().await.push(spec);

        let response = self.get_response(&hat_id);
        match response {
            JobResponse::SucceedWithEvents(events) => {
                let mut seq = 0u64;
                let mut pending = Vec::new();
                for (topic, payload) in events {
                    let event = Event::new(&topic, &payload, Some(&hat_id), &run_id, seq);
                    pending.push(event);
                    seq += 1;
                }
                self.pending_events
                    .lock()
                    .await
                    .insert(handle_id.clone(), pending);
            }
            JobResponse::Fail(_) => {
                self.pending_events
                    .lock()
                    .await
                    .insert(handle_id.clone(), Vec::new());
            }
        }

        Ok(JobHandle { id: handle_id })
    }

    async fn wait_job(&self, handle: &JobHandle) -> anyhow::Result<JobStatus> {
        // Extract hat_id: handle id is "{run_id}-{hat_id}" where run_id is "run-{uuid}"
        // So format is "run-XXXX-hat_id"
        let parts: Vec<&str> = handle.id.splitn(3, '-').collect();
        let hat_id = if parts.len() >= 3 { parts[2] } else { &handle.id };

        let response = self.get_response(hat_id);
        match response {
            JobResponse::SucceedWithEvents(_) => Ok(JobStatus::Succeeded {
                stdout: String::new(),
            }),
            JobResponse::Fail(reason) => Ok(JobStatus::Failed {
                reason,
                stdout: String::new(),
            }),
        }
    }

    async fn cancel_job(&self, _handle: &JobHandle) -> anyhow::Result<()> {
        Ok(())
    }
}
