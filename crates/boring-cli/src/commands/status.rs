use boring_store::{LocalStore, Store};
use std::process::ExitCode;

pub async fn run(run_id: Option<&str>) -> ExitCode {
    let store_dir = std::env::current_dir().unwrap().join(".boring");
    let store = LocalStore::new(&store_dir);

    // If no run_id specified, find the latest checkpoint
    let checkpoint = if let Some(rid) = run_id {
        boring_controller::checkpoint::Checkpoint::load(&store, rid).await
    } else {
        boring_controller::checkpoint::Checkpoint::find_latest(&store).await
    };

    match checkpoint {
        Ok(Some(cp)) => {
            println!("Run: {}", cp.run_id);
            println!("Iterations: {}", cp.iterations);
            println!("Sequence: {}", cp.global_sequence);
            println!("Consecutive failures: {}", cp.consecutive_failures);
            println!();
            println!("Hat activations:");
            for (hat, count) in &cp.activations {
                println!("  {:<20} {}", hat, count);
            }
            if !cp.seen_events.is_empty() {
                println!();
                println!("Events seen:");
                for event in &cp.seen_events {
                    println!("  - {}", event);
                }
            }
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("No run found.");
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("Error reading status: {}", e);
            ExitCode::from(1)
        }
    }
}
