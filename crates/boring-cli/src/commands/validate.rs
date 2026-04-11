use std::path::Path;
use std::process::ExitCode;
use boring_proto::config::BoringConfig;

pub fn run(config_path: &Path) -> ExitCode {
    let config = match BoringConfig::from_file(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            return ExitCode::from(1);
        }
    };

    match config.validate() {
        Ok(()) => {
            println!("Config is valid.");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            eprintln!("Validation errors:");
            for e in &errors {
                eprintln!("  - {}", e);
            }
            ExitCode::from(2)
        }
    }
}
