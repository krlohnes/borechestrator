use std::path::Path;
use std::process::ExitCode;
use boring_proto::config::{BoringConfig, RuntimeMode};
use boring_broker::NatsBroker;
use boring_store::LocalStore;
use boring_runtime::{LocalRuntime, DockerRuntime, Runtime};
use boring_secrets::{EnvSecretProvider, ChainSecretProvider, FileSecretProvider};
use boring_controller::reconciler::{Reconciler, RunResult};

pub async fn run(config_path: &Path, inline_prompt: Option<&str>, prompt_file: Option<&Path>, resume: bool, mode_override: Option<&str>) -> ExitCode {
    let mut config = match BoringConfig::from_file(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            return ExitCode::from(1);
        }
    };

    // Override prompt_file from CLI args
    if let Some(pf) = prompt_file {
        config.event_loop.prompt_file = Some(pf.to_string_lossy().to_string());
    }
    if let Some(inline) = inline_prompt {
        let tmp = std::env::temp_dir().join("boring-inline-prompt.md");
        if let Err(e) = std::fs::write(&tmp, inline) {
            eprintln!("Failed to write inline prompt: {}", e);
            return ExitCode::from(1);
        }
        config.event_loop.prompt_file = Some(tmp.to_string_lossy().to_string());
    }

    if let Err(errors) = config.validate() {
        eprintln!("Validation errors:");
        for e in &errors {
            eprintln!("  - {}", e);
        }
        return ExitCode::from(2);
    }

    // Determine runtime mode: CLI flag > config > default (local)
    let mode = mode_override
        .map(|m| match m {
            "local" => RuntimeMode::Local,
            "docker" => RuntimeMode::Docker,
            "k8s" => RuntimeMode::K8s,
            _ => RuntimeMode::Local,
        })
        .or_else(|| config.runtime.as_ref().map(|r| r.mode.clone()))
        .unwrap_or(RuntimeMode::Local);

    // Determine broker URL
    let broker_url = config
        .broker
        .as_ref()
        .map(|b| b.url.as_str())
        .unwrap_or("nats://127.0.0.1:4222");

    let stream_name = config
        .broker
        .as_ref()
        .and_then(|b| b.stream.as_deref())
        .unwrap_or("BORING");

    let broker = match NatsBroker::new(broker_url, stream_name).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to connect to NATS: {}", e);
            return ExitCode::from(1);
        }
    };

    // Store: always local filesystem for now
    let store_dir = std::env::current_dir().unwrap().join(".boring");
    let store = LocalStore::new(&store_dir);

    // Select runtime
    let runtime: Box<dyn Runtime> = match mode {
        RuntimeMode::Local => {
            println!("Runtime: local");
            Box::new(LocalRuntime::new())
        }
        RuntimeMode::Docker => {
            println!("Runtime: docker");
            let docker = DockerRuntime::new();
            Box::new(docker)
        }
        RuntimeMode::K8s => {
            println!("Runtime: k8s");
            let namespace = config.runtime.as_ref()
                .and_then(|r| r.namespace.as_deref())
                .unwrap_or("default");
            let default_image = config.runtime.as_ref()
                .and_then(|r| r.default_image.as_deref())
                .unwrap_or("alpine:latest");
            match boring_runtime::k8s::K8sRuntime::new(namespace, default_image).await {
                Ok(rt) => Box::new(rt),
                Err(e) => {
                    eprintln!("Failed to connect to K8s: {}", e);
                    return ExitCode::from(1);
                }
            }
        }
    };

    // Secret resolution chain
    let secrets_dir = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join(".boring")
        .join("secrets");
    let secrets = ChainSecretProvider::new(vec![
        Box::new(EnvSecretProvider::new()),
        Box::new(FileSecretProvider::new(&secrets_dir)),
    ]);

    let mut reconciler = Reconciler::new(
        config,
        Box::new(broker),
        Box::new(store),
        runtime,
        Box::new(secrets),
    );

    let result = if resume {
        println!("Resuming from last checkpoint...");
        reconciler.resume().await
    } else {
        reconciler.run().await
    };

    match result {
        Ok(RunResult::Completed) => {
            println!("Run completed successfully.");
            ExitCode::SUCCESS
        }
        Ok(RunResult::MaxIterationsReached) => {
            println!("Run reached maximum iterations.");
            ExitCode::from(1)
        }
        Ok(RunResult::TimedOut) => {
            println!("Run timed out.");
            ExitCode::from(1)
        }
        Ok(RunResult::Failed { reason }) => {
            eprintln!("Run failed: {}", reason);
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}
