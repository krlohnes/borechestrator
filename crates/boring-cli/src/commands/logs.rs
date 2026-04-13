use boring_store::{LocalStore, Store};
use std::process::ExitCode;

pub async fn run(run_id: &str, hat: Option<&str>) -> ExitCode {
    let store_dir = std::env::current_dir().unwrap().join(".boring");
    let store = LocalStore::new(&store_dir);

    // List event archive files for this run
    let prefix = format!("{}/events/", run_id);
    let keys = match store.list(&prefix).await {
        Ok(k) => k,
        Err(e) => {
            eprintln!("Error listing events: {}", e);
            return ExitCode::from(1);
        }
    };

    if keys.is_empty() {
        // Try reading scratchpads as a fallback view
        let sp_prefix = format!("{}/scratchpad/", run_id);
        let sp_keys = store.list(&sp_prefix).await.unwrap_or_default();

        if sp_keys.is_empty() {
            eprintln!("No logs or scratchpads found for run {}", run_id);
            return ExitCode::from(1);
        }

        println!("Scratchpads for run {}:\n", run_id);
        for key in &sp_keys {
            if let Some(hat_filter) = hat {
                if !key.contains(hat_filter) {
                    continue;
                }
            }
            let filename = key.rsplit('/').next().unwrap_or(key);
            println!("── {} ──", filename);
            if let Ok(Some(bytes)) = store.get(key).await {
                println!("{}", String::from_utf8_lossy(&bytes));
            }
            println!();
        }
        return ExitCode::SUCCESS;
    }

    // Show event log
    println!("Events for run {}:\n", run_id);
    for key in &keys {
        if let Ok(Some(bytes)) = store.get(key).await {
            if let Ok(event) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                let topic = event.get("topic").and_then(|v| v.as_str()).unwrap_or("?");
                let source = event.get("source").and_then(|v| v.as_str()).unwrap_or("-");
                let payload = event.get("payload").and_then(|v| v.as_str()).unwrap_or("");

                if let Some(hat_filter) = hat {
                    if source != hat_filter {
                        continue;
                    }
                }

                let seq = event.get("sequence").and_then(|v| v.as_u64()).unwrap_or(0);
                println!("[{:>3}] {:<30} ({}) {}", seq, topic, source, payload);
            }
        }
    }

    ExitCode::SUCCESS
}
