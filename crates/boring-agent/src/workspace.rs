use anyhow::Context;
use boring_store::Store;
use std::path::Path;
use tracing::info;

/// Materialize all S3 state into a local `.boring/` directory so the
/// agent's AI CLI can grep, read, and search it natively.
pub async fn materialize(
    store: &dyn Store,
    run_id: &str,
    hat_id: &str,
    work_dir: &Path,
    prompt: &str,
    event_topic: &str,
    event_payload: &str,
) -> anyhow::Result<()> {
    // Ensure .boring/ is gitignored — state syncs via S3, not git
    let gitignore_path = work_dir.join(".gitignore");
    let gitignore = tokio::fs::read_to_string(&gitignore_path)
        .await
        .unwrap_or_default();
    if !gitignore.contains(".boring/") {
        tokio::fs::write(&gitignore_path, format!("{}\n.boring/\n", gitignore.trim())).await?;
    }

    let boring_dir = work_dir.join(".boring");
    tokio::fs::create_dir_all(&boring_dir)
        .await
        .context("failed to create .boring/")?;
    tokio::fs::create_dir_all(boring_dir.join("scratchpad"))
        .await
        .context("failed to create .boring/scratchpad/")?;

    // Write the prompt so the agent can reference it
    tokio::fs::write(boring_dir.join("prompt.md"), prompt).await?;
    info!("wrote .boring/prompt.md");

    // Write event context
    let event_json = serde_json::json!({
        "topic": event_topic,
        "payload": event_payload,
        "run_id": run_id,
        "hat_id": hat_id,
    });
    tokio::fs::write(
        boring_dir.join("event.json"),
        serde_json::to_string_pretty(&event_json)?,
    )
    .await?;
    info!("wrote .boring/event.json");

    // Download this hat's scratchpad
    let scratchpad_key = format!("{}/scratchpad/{}.md", run_id, hat_id);
    if let Some(bytes) = store.get(&scratchpad_key).await? {
        tokio::fs::write(
            boring_dir.join("scratchpad").join(format!("{}.md", hat_id)),
            &bytes,
        )
        .await?;
        info!(
            "wrote .boring/scratchpad/{}.md ({} bytes)",
            hat_id,
            bytes.len()
        );
    }

    // Download shared scratchpad
    let shared_key = format!("{}/scratchpad/shared.md", run_id);
    if let Some(bytes) = store.get(&shared_key).await? {
        tokio::fs::write(boring_dir.join("scratchpad").join("shared.md"), &bytes).await?;
        info!("wrote .boring/scratchpad/shared.md");
    }

    // Download memories and render as markdown for grep
    let memories_key = format!("{}/memories.json", run_id);
    if let Some(bytes) = store.get(&memories_key).await? {
        // Write raw JSON for programmatic access
        tokio::fs::write(boring_dir.join("memories.json"), &bytes).await?;

        // Also render as readable markdown
        if let Ok(memories) = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes) {
            let mut md = String::from("# Memories\n\n");
            for mem in &memories {
                let mtype = mem
                    .get("memory_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let content = mem.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let source = mem.get("source").and_then(|v| v.as_str()).unwrap_or("?");
                md.push_str(&format!("- **[{}]** ({}): {}\n", mtype, source, content));
            }
            tokio::fs::write(boring_dir.join("memories.md"), &md).await?;
            info!("wrote .boring/memories.md ({} entries)", memories.len());
        }
    }

    // Download tasks and render as markdown
    let tasks_key = format!("{}/tasks.json", run_id);
    if let Some(bytes) = store.get(&tasks_key).await? {
        tokio::fs::write(boring_dir.join("tasks.json"), &bytes).await?;

        if let Ok(tasks) = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes) {
            let mut md = String::from("# Tasks\n\n");
            for task in &tasks {
                let status = task.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                let title = task.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                let icon = match status {
                    "pending" => "[ ]",
                    "in_progress" => "[~]",
                    "done" => "[x]",
                    "blocked" => "[!]",
                    _ => "[?]",
                };
                md.push_str(&format!("{} {}\n", icon, title));
            }
            tokio::fs::write(boring_dir.join("tasks.md"), &md).await?;
            info!("wrote .boring/tasks.md ({} tasks)", tasks.len());
        }
    }

    // Download any other scratchpads (from other hats, for cross-hat visibility)
    let scratchpad_prefix = format!("{}/scratchpad/", run_id);
    if let Ok(keys) = store.list(&scratchpad_prefix).await {
        for key in keys {
            let filename = key.strip_prefix(&scratchpad_prefix).unwrap_or(&key);
            // Skip ones we already downloaded
            if filename == format!("{}.md", hat_id) || filename == "shared.md" {
                continue;
            }
            if let Some(bytes) = store.get(&key).await? {
                tokio::fs::write(boring_dir.join("scratchpad").join(filename), &bytes).await?;
                info!("wrote .boring/scratchpad/{}", filename);
            }
        }
    }

    info!("workspace materialized in .boring/");
    Ok(())
}

/// Sync local `.boring/` state back to S3 after the agent finishes.
pub async fn sync_back(
    store: &dyn Store,
    run_id: &str,
    hat_id: &str,
    work_dir: &Path,
) -> anyhow::Result<()> {
    let boring_dir = work_dir.join(".boring");

    // Upload this hat's scratchpad if it was modified
    let scratchpad_path = boring_dir.join("scratchpad").join(format!("{}.md", hat_id));
    if scratchpad_path.exists() {
        let content = tokio::fs::read(&scratchpad_path).await?;
        let key = format!("{}/scratchpad/{}.md", run_id, hat_id);
        store.put(&key, content).await?;
        info!("synced .boring/scratchpad/{}.md to S3", hat_id);
    }

    // Upload shared scratchpad if it exists
    let shared_path = boring_dir.join("scratchpad").join("shared.md");
    if shared_path.exists() {
        let content = tokio::fs::read(&shared_path).await?;
        let key = format!("{}/scratchpad/shared.md", run_id);
        store.put(&key, content).await?;
        info!("synced .boring/scratchpad/shared.md to S3");
    }

    // Upload memories if modified
    let memories_path = boring_dir.join("memories.json");
    if memories_path.exists() {
        let content = tokio::fs::read(&memories_path).await?;
        let key = format!("{}/memories.json", run_id);
        store.put(&key, content).await?;
        info!("synced .boring/memories.json to S3");
    }

    // Upload tasks if modified
    let tasks_path = boring_dir.join("tasks.json");
    if tasks_path.exists() {
        let content = tokio::fs::read(&tasks_path).await?;
        let key = format!("{}/tasks.json", run_id);
        store.put(&key, content).await?;
        info!("synced .boring/tasks.json to S3");
    }

    info!("workspace synced back to S3");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boring_store::LocalStore;
    use tempfile::TempDir;

    async fn setup() -> (LocalStore, TempDir, TempDir) {
        let store_dir = TempDir::new().unwrap();
        let work_dir = TempDir::new().unwrap();
        let store = LocalStore::new(store_dir.path());
        (store, store_dir, work_dir)
    }

    #[tokio::test]
    async fn test_materialize_creates_boring_dir() {
        let (store, _sd, wd) = setup().await;
        materialize(
            &store,
            "run-1",
            "planner",
            wd.path(),
            "do stuff",
            "work.start",
            "go",
        )
        .await
        .unwrap();

        assert!(wd.path().join(".boring").exists());
        assert!(wd.path().join(".boring/prompt.md").exists());
        assert!(wd.path().join(".boring/event.json").exists());
    }

    #[tokio::test]
    async fn test_materialize_writes_prompt() {
        let (store, _sd, wd) = setup().await;
        materialize(
            &store,
            "run-1",
            "planner",
            wd.path(),
            "build the thing",
            "work.start",
            "go",
        )
        .await
        .unwrap();

        let content = std::fs::read_to_string(wd.path().join(".boring/prompt.md")).unwrap();
        assert!(content.contains("build the thing"));
    }

    #[tokio::test]
    async fn test_materialize_downloads_scratchpad() {
        let (store, _sd, wd) = setup().await;
        store
            .put(
                "run-1/scratchpad/planner.md",
                b"## Progress\n- step 1".to_vec(),
            )
            .await
            .unwrap();

        materialize(&store, "run-1", "planner", wd.path(), "prompt", "t", "p")
            .await
            .unwrap();

        let content =
            std::fs::read_to_string(wd.path().join(".boring/scratchpad/planner.md")).unwrap();
        assert!(content.contains("step 1"));
    }

    #[tokio::test]
    async fn test_materialize_renders_memories_as_markdown() {
        let (store, _sd, wd) = setup().await;
        let memories = serde_json::json!([
            {"memory_type": "pattern", "content": "Use snake_case", "source": "builder", "timestamp": "2026-01-01"},
            {"memory_type": "decision", "content": "Chose NATS", "source": "planner", "timestamp": "2026-01-01"},
        ]);
        store
            .put(
                "run-1/memories.json",
                serde_json::to_vec(&memories).unwrap(),
            )
            .await
            .unwrap();

        materialize(&store, "run-1", "planner", wd.path(), "prompt", "t", "p")
            .await
            .unwrap();

        let md = std::fs::read_to_string(wd.path().join(".boring/memories.md")).unwrap();
        assert!(md.contains("[pattern]"));
        assert!(md.contains("Use snake_case"));
        assert!(md.contains("[decision]"));

        // JSON also available
        assert!(wd.path().join(".boring/memories.json").exists());
    }

    #[tokio::test]
    async fn test_materialize_renders_tasks_as_markdown() {
        let (store, _sd, wd) = setup().await;
        let tasks = serde_json::json!([
            {"id": "1", "title": "Write tests", "status": "done", "created_by": "planner", "timestamp": "2026-01-01", "depends_on": []},
            {"id": "2", "title": "Implement feature", "status": "in_progress", "created_by": "planner", "timestamp": "2026-01-01", "depends_on": []},
        ]);
        store
            .put("run-1/tasks.json", serde_json::to_vec(&tasks).unwrap())
            .await
            .unwrap();

        materialize(&store, "run-1", "planner", wd.path(), "prompt", "t", "p")
            .await
            .unwrap();

        let md = std::fs::read_to_string(wd.path().join(".boring/tasks.md")).unwrap();
        assert!(md.contains("[x] Write tests"));
        assert!(md.contains("[~] Implement feature"));
    }

    #[tokio::test]
    async fn test_materialize_downloads_other_hat_scratchpads() {
        let (store, _sd, wd) = setup().await;
        store
            .put("run-1/scratchpad/builder.md", b"builder notes".to_vec())
            .await
            .unwrap();
        store
            .put("run-1/scratchpad/planner.md", b"planner notes".to_vec())
            .await
            .unwrap();

        materialize(&store, "run-1", "planner", wd.path(), "prompt", "t", "p")
            .await
            .unwrap();

        // Own scratchpad
        assert!(wd.path().join(".boring/scratchpad/planner.md").exists());
        // Other hat's scratchpad (cross-hat visibility)
        assert!(wd.path().join(".boring/scratchpad/builder.md").exists());
        let builder =
            std::fs::read_to_string(wd.path().join(".boring/scratchpad/builder.md")).unwrap();
        assert!(builder.contains("builder notes"));
    }

    #[tokio::test]
    async fn test_sync_back_uploads_scratchpad() {
        let (store, _sd, wd) = setup().await;

        // Materialize first
        materialize(&store, "run-1", "planner", wd.path(), "prompt", "t", "p")
            .await
            .unwrap();

        // Agent writes to scratchpad
        std::fs::write(
            wd.path().join(".boring/scratchpad/planner.md"),
            "## Updated\n- step 2 done",
        )
        .unwrap();

        // Sync back
        sync_back(&store, "run-1", "planner", wd.path())
            .await
            .unwrap();

        // Verify S3 has the update
        let content = store
            .get("run-1/scratchpad/planner.md")
            .await
            .unwrap()
            .unwrap();
        assert!(String::from_utf8_lossy(&content).contains("step 2 done"));
    }

    #[tokio::test]
    async fn test_sync_back_uploads_memories() {
        let (store, _sd, wd) = setup().await;

        materialize(&store, "run-1", "planner", wd.path(), "prompt", "t", "p")
            .await
            .unwrap();

        // Agent creates a memories file
        let memories = serde_json::json!([
            {"memory_type": "fix", "content": "null check needed", "source": "builder", "timestamp": "2026-01-01"}
        ]);
        std::fs::write(
            wd.path().join(".boring/memories.json"),
            serde_json::to_string(&memories).unwrap(),
        )
        .unwrap();

        sync_back(&store, "run-1", "planner", wd.path())
            .await
            .unwrap();

        let content = store.get("run-1/memories.json").await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&content).contains("null check"));
    }
}
