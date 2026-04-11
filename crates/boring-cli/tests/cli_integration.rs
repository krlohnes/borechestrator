use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn write_config(dir: &TempDir, content: &str) -> std::path::PathBuf {
    let path = dir.path().join("borechestrator.yml");
    fs::write(&path, content).unwrap();
    path
}

fn boring_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_boring-cli"))
}

#[test]
fn test_validate_valid_config() {
    let dir = TempDir::new().unwrap();
    let config_path = write_config(
        &dir,
        r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
hats:
  worker:
    name: Worker
    description: "Does work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
"#,
    );

    let output = boring_cmd()
        .args(["validate", "-c", config_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_validate_invalid_config_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    let config_path = write_config(
        &dir,
        r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
hats:
  worker:
    name: Worker
    description: "Does work"
    triggers: []
    publishes: ["work.done"]
    instructions: "Do it."
"#,
    );

    let output = boring_cmd()
        .args(["validate", "-c", config_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn test_validate_missing_file_exits_nonzero() {
    let output = boring_cmd()
        .args(["validate", "-c", "/nonexistent/path.yml"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}
