use std::env;
use std::process::ExitCode;

/// Tiny CLI for agents to emit events from inside containers.
///
/// Usage:
///   emit <topic> [payload]
///   emit --complete
///   emit --memory <type> <content>
///   emit --task add <title>
///   emit --task done <id>
///   emit --scratchpad <content>
///
/// Writes to a well-known file that boring-agent reads after the command finishes.
fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("Usage: emit <topic> [payload]");
        eprintln!("       emit --complete");
        eprintln!("       emit --memory <type> <content>");
        eprintln!("       emit --task add|done <arg>");
        eprintln!("       emit --scratchpad <content>");
        return ExitCode::from(1);
    }

    // All emits go to a file that boring-agent reads
    let emit_file = env::var("BORING_EMIT_FILE")
        .unwrap_or_else(|_| "/tmp/boring-emits.jsonl".to_string());

    let line = match args[0].as_str() {
        "--complete" => {
            let promise = env::var("BORING_COMPLETION_PROMISE")
                .unwrap_or_else(|_| "LOOP_COMPLETE".to_string());
            format!(r#"{{"type":"complete","promise":"{}"}}"#, promise)
        }
        "--memory" => {
            if args.len() < 3 {
                eprintln!("Usage: emit --memory <type> <content>");
                return ExitCode::from(1);
            }
            let mtype = &args[1];
            let content = args[2..].join(" ");
            format!(r#"{{"type":"memory","memory_type":"{}","content":"{}"}}"#, mtype, content.replace('"', "\\\""))
        }
        "--task" => {
            if args.len() < 3 {
                eprintln!("Usage: emit --task add|done <arg>");
                return ExitCode::from(1);
            }
            let action = &args[1];
            let arg = args[2..].join(" ");
            format!(r#"{{"type":"task","action":"{}","arg":"{}"}}"#, action, arg.replace('"', "\\\""))
        }
        "--scratchpad" => {
            let content = args[1..].join(" ");
            format!(r#"{{"type":"scratchpad","content":"{}"}}"#, content.replace('"', "\\\""))
        }
        topic => {
            let payload = if args.len() > 1 { args[1..].join(" ") } else { String::new() };
            format!(r#"{{"type":"event","topic":"{}","payload":"{}"}}"#, topic, payload.replace('"', "\\\""))
        }
    };

    // Append to the emit file
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&emit_file)
    {
        Ok(mut f) => {
            if writeln!(f, "{}", line).is_err() {
                eprintln!("Failed to write to {}", emit_file);
                return ExitCode::from(1);
            }
        }
        Err(e) => {
            eprintln!("Failed to open {}: {}", emit_file, e);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}
