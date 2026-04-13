use boring_store::Store;
use serde::{Deserialize, Serialize};

/// A work item tracked during orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub priority: Option<u32>,
    pub assigned_to: Option<String>,
    pub depends_on: Vec<String>,
    pub created_by: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
}

/// Manages tasks stored in the backing store.
pub struct TaskStore {
    store_key: String,
}

impl TaskStore {
    pub fn new(run_id: &str) -> Self {
        Self {
            store_key: format!("{}/tasks.json", run_id),
        }
    }

    pub async fn load(&self, store: &dyn Store) -> anyhow::Result<Vec<Task>> {
        match store.get(&self.store_key).await? {
            Some(bytes) => {
                let tasks: Vec<Task> = serde_json::from_slice(&bytes)?;
                Ok(tasks)
            }
            None => Ok(Vec::new()),
        }
    }

    pub async fn save(&self, store: &dyn Store, tasks: &[Task]) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec_pretty(tasks)?;
        store.put(&self.store_key, bytes).await?;
        Ok(())
    }

    pub async fn add(&self, store: &dyn Store, task: Task) -> anyhow::Result<()> {
        let mut tasks = self.load(store).await?;
        tasks.push(task);
        self.save(store, &tasks).await?;
        Ok(())
    }

    pub async fn update_status(
        &self,
        store: &dyn Store,
        task_id: &str,
        status: TaskStatus,
    ) -> anyhow::Result<()> {
        let mut tasks = self.load(store).await?;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = status;
        }
        self.save(store, &tasks).await?;
        Ok(())
    }

    /// Format tasks for injection into prompts.
    pub fn format_for_prompt(tasks: &[Task]) -> String {
        if tasks.is_empty() {
            return String::new();
        }

        let mut lines = vec!["# Tasks".to_string(), String::new()];
        for task in tasks {
            let status_icon = match task.status {
                TaskStatus::Pending => "[ ]",
                TaskStatus::InProgress => "[~]",
                TaskStatus::Done => "[x]",
                TaskStatus::Blocked => "[!]",
            };
            let assignee = task
                .assigned_to
                .as_deref()
                .map(|a| format!(" ({})", a))
                .unwrap_or_default();
            lines.push(format!("{} {}{}", status_icon, task.title, assignee));
        }

        lines.join("\n")
    }
}

/// Parse a BORING_TASK line from agent stdout.
/// Format: BORING_TASK <add|done> <id_or_title>
pub fn parse_task_line(line: &str, source: &str) -> Option<TaskAction> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("BORING_TASK ")?;
    let mut parts = rest.splitn(2, ' ');
    let action = parts.next()?;
    let arg = parts.next().unwrap_or("");

    match action {
        "add" => Some(TaskAction::Add(Task {
            id: format!(
                "task-{}",
                uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
            ),
            title: arg.to_string(),
            status: TaskStatus::Pending,
            priority: None,
            assigned_to: None,
            depends_on: Vec::new(),
            created_by: source.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        })),
        "done" => Some(TaskAction::Done(arg.to_string())),
        "progress" => Some(TaskAction::InProgress(arg.to_string())),
        _ => None,
    }
}

#[derive(Debug)]
pub enum TaskAction {
    Add(Task),
    Done(String),       // task id
    InProgress(String), // task id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_task_add() {
        let action =
            parse_task_line("BORING_TASK add Implement user authentication", "planner").unwrap();
        match action {
            TaskAction::Add(task) => {
                assert_eq!(task.title, "Implement user authentication");
                assert_eq!(task.created_by, "planner");
                assert_eq!(task.status, TaskStatus::Pending);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_task_done() {
        let action = parse_task_line("BORING_TASK done task-abc123", "builder").unwrap();
        match action {
            TaskAction::Done(id) => assert_eq!(id, "task-abc123"),
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn test_parse_task_not_a_task() {
        assert!(parse_task_line("regular output", "worker").is_none());
    }

    #[test]
    fn test_format_for_prompt() {
        let tasks = vec![
            Task {
                id: "1".to_string(),
                title: "Write tests".to_string(),
                status: TaskStatus::Done,
                priority: None,
                assigned_to: Some("builder".to_string()),
                depends_on: Vec::new(),
                created_by: "planner".to_string(),
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            },
            Task {
                id: "2".to_string(),
                title: "Implement feature".to_string(),
                status: TaskStatus::InProgress,
                priority: None,
                assigned_to: None,
                depends_on: Vec::new(),
                created_by: "planner".to_string(),
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            },
        ];
        let result = TaskStore::format_for_prompt(&tasks);
        assert!(result.contains("[x] Write tests (builder)"));
        assert!(result.contains("[~] Implement feature"));
    }
}
