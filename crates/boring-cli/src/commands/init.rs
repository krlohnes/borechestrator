use std::process::ExitCode;

const PRESETS: &[(&str, &str, &str)] = &[
    ("feature", "Feature development with planner, builder, and reviewer", FEATURE_PRESET),
    ("tdd", "Test-driven development with red-green-refactor cycle", TDD_PRESET),
    ("research", "Research and analysis (no code changes)", RESEARCH_PRESET),
    ("debug", "Hypothesis-driven debugging", DEBUG_PRESET),
    ("review", "Adversarial code review (red team / blue team)", REVIEW_PRESET),
    ("minimal", "Single agent that completes a task", MINIMAL_PRESET),
];

pub fn run(preset: Option<&str>, list: bool) -> ExitCode {
    if list {
        println!("Available presets:\n");
        for (name, description, _) in PRESETS {
            println!("  {:<12} {}", name, description);
        }
        return ExitCode::SUCCESS;
    }

    let preset_name = preset.unwrap_or("minimal");
    let template = PRESETS
        .iter()
        .find(|(name, _, _)| *name == preset_name)
        .map(|(_, _, content)| *content);

    let Some(template) = template else {
        eprintln!("Unknown preset: {}", preset_name);
        eprintln!("Run `boring init --list` to see available presets.");
        return ExitCode::from(1);
    };

    let output_path = "borechestrator.yml";
    if std::path::Path::new(output_path).exists() {
        eprintln!("{} already exists. Remove it first or use a different name.", output_path);
        return ExitCode::from(1);
    }

    std::fs::write(output_path, template).unwrap();
    println!("Created {} (preset: {})", output_path, preset_name);
    ExitCode::SUCCESS
}

const MINIMAL_PRESET: &str = r#"# Borechestrator — Minimal preset
# A single agent that completes a task.

event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 20

hats:
  worker:
    name: Worker
    description: "Completes the assigned task"
    triggers: ["work.start"]
    publishes: []
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Complete the task described in the event payload.
      When done, output LOOP_COMPLETE.
"#;

const FEATURE_PRESET: &str = r#"# Borechestrator — Feature development preset
# Planner breaks work down, builder implements, reviewer validates.

event_loop:
  starting_event: build.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 30

hats:
  planner:
    name: Planner
    description: "Analyzes requirements and creates sub-tasks"
    triggers: ["build.start", "task.complete"]
    publishes: ["tasks.ready"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Read the requirements. Break work into specific sub-tasks.
      For each sub-task, emit: BORING_EMIT tasks.ready <description>
      If all tasks are done, output LOOP_COMPLETE.

  builder:
    name: Builder
    description: "Implements one sub-task at a time"
    triggers: ["tasks.ready", "review.rejected"]
    publishes: ["review.ready"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Implement the sub-task from the event payload.
      Run tests to verify. Commit your changes.
      When done, emit: BORING_EMIT review.ready <summary>
      If review.rejected, address the feedback and resubmit.

  reviewer:
    name: Reviewer
    description: "Reviews implementation quality"
    triggers: ["review.ready"]
    publishes: ["review.rejected", "task.complete"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Review the implementation:
      - Correctness and edge cases
      - Code quality and style
      - Test coverage

      If issues found: BORING_EMIT review.rejected <feedback>
      If approved: BORING_EMIT task.complete <summary>
"#;

const TDD_PRESET: &str = r#"# Borechestrator — TDD preset
# Red-green-refactor cycle.

event_loop:
  starting_event: tdd.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 30

hats:
  test_writer:
    name: Test Writer
    description: "Writes FAILING tests first"
    triggers: ["tdd.start", "refactor.done"]
    publishes: ["test.written"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Write a FAILING test for the next requirement.
      Run the test to verify it FAILS (red phase).
      NEVER write implementation code.
      Emit: BORING_EMIT test.written <test description>
      If all requirements are tested, output LOOP_COMPLETE.

  implementer:
    name: Implementer
    description: "Makes failing tests pass with minimal code"
    triggers: ["test.written"]
    publishes: ["test.passing"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Make the failing test pass with MINIMAL code.
      Do NOT refactor. Do NOT add extra functionality.
      Run tests to confirm green.
      Emit: BORING_EMIT test.passing <summary>

  refactorer:
    name: Refactorer
    description: "Cleans up code while keeping tests green"
    triggers: ["test.passing"]
    publishes: ["refactor.done"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Review for code smells. Refactor for clarity.
      Run tests to confirm still passing.
      Emit: BORING_EMIT refactor.done <what changed>
"#;

const RESEARCH_PRESET: &str = r#"# Borechestrator — Research preset
# Gather information and synthesize findings. No code changes.

event_loop:
  starting_event: research.start
  completion_promise: RESEARCH_COMPLETE
  max_iterations: 15

hats:
  researcher:
    name: Researcher
    description: "Gathers information and analyzes patterns"
    triggers: ["research.start", "research.followup"]
    publishes: ["research.finding"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Research the topic. Search broadly, read files, gather evidence.
      Document findings with file:line references.
      DO NOT write code or make commits.
      Emit: BORING_EMIT research.finding <findings summary>

  synthesizer:
    name: Synthesizer
    description: "Reviews findings and creates coherent summary"
    triggers: ["research.finding"]
    publishes: ["research.followup"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Review all findings. Identify patterns and gaps.
      If gaps exist: BORING_EMIT research.followup <questions>
      If complete: output RESEARCH_COMPLETE with a summary.
"#;

const DEBUG_PRESET: &str = r#"# Borechestrator — Debug preset
# Scientific method debugging: observe, hypothesize, test, fix.

event_loop:
  starting_event: debug.start
  completion_promise: DEBUG_COMPLETE
  max_iterations: 20

hats:
  observer:
    name: Observer
    description: "Gathers symptoms and evidence"
    triggers: ["debug.start", "hypothesis.rejected"]
    publishes: ["observation.made"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Gather evidence: logs, stack traces, error messages.
      Note what works vs what doesn't.
      DO NOT guess at fixes or modify code.
      Emit: BORING_EMIT observation.made <evidence>

  theorist:
    name: Theorist
    description: "Forms testable hypotheses"
    triggers: ["observation.made"]
    publishes: ["hypothesis.formed"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Review evidence. Identify possible root causes.
      Formulate a testable hypothesis.
      Describe how to confirm or reject it.
      Emit: BORING_EMIT hypothesis.formed <hypothesis and test plan>

  experimenter:
    name: Experimenter
    description: "Tests hypotheses"
    triggers: ["hypothesis.formed"]
    publishes: ["hypothesis.confirmed", "hypothesis.rejected"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Follow the test plan. Run the experiment.
      If confirmed: BORING_EMIT hypothesis.confirmed <evidence>
      If rejected: BORING_EMIT hypothesis.rejected <what was ruled out>

  fixer:
    name: Fixer
    description: "Fixes confirmed root cause"
    triggers: ["hypothesis.confirmed"]
    publishes: []
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Fix the confirmed root cause.
      Add a regression test. Verify the original issue is resolved.
      Output DEBUG_COMPLETE.
"#;

const REVIEW_PRESET: &str = r#"# Borechestrator — Adversarial review preset
# Red team tries to break it, blue team fixes it.

event_loop:
  starting_event: security.review
  completion_promise: LOOP_COMPLETE
  max_iterations: 20

hats:
  blue_team:
    name: Blue Team
    description: "Implements with security in mind"
    triggers: ["security.review", "vulnerability.found"]
    publishes: ["build.ready"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Review or fix the code with security in mind.
      Consider: input validation, injection, auth, data exposure.
      If vulnerability.found, fix the issue and add a regression test.
      Emit: BORING_EMIT build.ready <summary>

  red_team:
    name: Red Team
    description: "Tries to break the code"
    triggers: ["build.ready"]
    publishes: ["vulnerability.found"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Your job is to BREAK this code. Attack vectors:
      - Injection (SQL, XSS, command)
      - Auth bypass, IDOR
      - Race conditions, path traversal

      If vulnerabilities found: BORING_EMIT vulnerability.found <details>
      If secure: output LOOP_COMPLETE
"#;
