use async_trait::async_trait;

/// Trait for human-in-the-loop interaction.
/// Implementations can use stdin, Slack, Telegram, webhooks, etc.
#[async_trait]
pub trait HumanInteraction: Send + Sync {
    /// Ask the human a question and wait for a response.
    async fn ask(&self, question: &str, context: &HumanContext) -> anyhow::Result<String>;

    /// Send a notification (no response expected).
    async fn notify(&self, message: &str, context: &HumanContext) -> anyhow::Result<()>;
}

pub struct HumanContext {
    pub run_id: String,
    pub hat_id: String,
    pub iteration: u32,
}

/// CLI-based human interaction via stdin/stdout.
pub struct CliHumanInteraction;

#[async_trait]
impl HumanInteraction for CliHumanInteraction {
    async fn ask(&self, question: &str, context: &HumanContext) -> anyhow::Result<String> {
        println!(
            "\n[HUMAN INPUT REQUIRED] (run: {}, hat: {}, iteration: {})",
            context.run_id, context.hat_id, context.iteration
        );
        println!("{}", question);
        print!("> ");

        let mut input = String::new();
        tokio::task::spawn_blocking(move || std::io::stdin().read_line(&mut input).map(|_| input))
            .await?
            .map_err(|e| anyhow::anyhow!("stdin read failed: {}", e))
            .map(|s| s.trim().to_string())
    }

    async fn notify(&self, message: &str, context: &HumanContext) -> anyhow::Result<()> {
        println!(
            "\n[NOTIFICATION] (run: {}, hat: {}): {}",
            context.run_id, context.hat_id, message
        );
        Ok(())
    }
}

/// No-op human interaction that auto-approves everything.
/// Used when human-in-the-loop is not configured.
pub struct AutoApproveInteraction;

#[async_trait]
impl HumanInteraction for AutoApproveInteraction {
    async fn ask(&self, _question: &str, _context: &HumanContext) -> anyhow::Result<String> {
        Ok("approved".to_string())
    }

    async fn notify(&self, _message: &str, _context: &HumanContext) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Parse a BORING_HUMAN line from agent stdout.
/// Format: BORING_HUMAN <question>
pub fn parse_human_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    trimmed.strip_prefix("BORING_HUMAN ").map(|s| s.to_string())
}

/// Parse a BORING_NOTIFY line from agent stdout.
/// Format: BORING_NOTIFY <message>
pub fn parse_notify_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix("BORING_NOTIFY ")
        .map(|s| s.to_string())
}

/// Represents a parsed human interaction event from stdout.
#[derive(Debug)]
pub enum HumanAction {
    Ask(String),
    Notify(String),
}

/// Parse all human interaction lines from stdout.
pub fn parse_human_actions(stdout: &str) -> Vec<HumanAction> {
    let mut actions = Vec::new();
    for line in stdout.lines() {
        if let Some(q) = parse_human_line(line) {
            actions.push(HumanAction::Ask(q));
        } else if let Some(m) = parse_notify_line(line) {
            actions.push(HumanAction::Notify(m));
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_human_line() {
        assert_eq!(
            parse_human_line("BORING_HUMAN Should I proceed with the migration?"),
            Some("Should I proceed with the migration?".to_string())
        );
    }

    #[test]
    fn test_parse_human_line_not_human() {
        assert!(parse_human_line("BORING_EMIT work.done").is_none());
        assert!(parse_human_line("regular output").is_none());
    }

    #[test]
    fn test_parse_notify_line() {
        assert_eq!(
            parse_notify_line("BORING_NOTIFY Build completed successfully"),
            Some("Build completed successfully".to_string())
        );
    }

    #[test]
    fn test_parse_human_actions() {
        let stdout = "doing work\nBORING_HUMAN Approve deployment?\nmore work\nBORING_NOTIFY Done building\n";
        let actions = parse_human_actions(stdout);
        assert_eq!(actions.len(), 2);
        match &actions[0] {
            HumanAction::Ask(q) => assert_eq!(q, "Approve deployment?"),
            _ => panic!("expected Ask"),
        }
        match &actions[1] {
            HumanAction::Notify(m) => assert_eq!(m, "Done building"),
            _ => panic!("expected Notify"),
        }
    }

    #[tokio::test]
    async fn test_auto_approve() {
        let human = AutoApproveInteraction;
        let ctx = HumanContext {
            run_id: "test".to_string(),
            hat_id: "worker".to_string(),
            iteration: 1,
        };
        let response = human.ask("proceed?", &ctx).await.unwrap();
        assert_eq!(response, "approved");
    }
}
