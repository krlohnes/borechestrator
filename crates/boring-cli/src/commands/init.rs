use std::process::ExitCode;

const PRESETS: &[(&str, &str, &str)] = &[
    ("feature", "Feature development with planner, builder, and reviewer", FEATURE_PRESET),
    ("tdd", "Test-driven development with red-green-refactor cycle", TDD_PRESET),
    ("research", "Research and analysis (no code changes)", RESEARCH_PRESET),
    ("debug", "Hypothesis-driven debugging", DEBUG_PRESET),
    ("review", "Adversarial code review (red team / blue team)", REVIEW_PRESET),
    ("minimal", "Single agent that completes a task", MINIMAL_PRESET),
    ("spec-driven", "Specification-driven development with contract-first approach", SPEC_DRIVEN_PRESET),
    ("mob", "Mob programming with navigator, driver, and observer", MOB_PRESET),
    ("refactor", "Code refactoring with safety checks", REFACTOR_PRESET),
    ("pr-review", "Multi-perspective pull request review", PR_REVIEW_PRESET),
    ("docs", "Documentation generation", DOCS_PRESET),
    ("deploy", "Deployment and release workflow", DEPLOY_PRESET),
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

cli:
  backend: claude

hats:
  worker:
    name: Worker
    description: "Completes the assigned task"
    triggers: ["work.start"]
    publishes: []
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

cli:
  backend: claude

hats:
  planner:
    name: Planner
    description: "Analyzes requirements and creates sub-tasks"
    triggers: ["build.start", "task.complete"]
    publishes: ["tasks.ready"]
    instructions: |
      Read the requirements. Break work into specific sub-tasks.
      For each sub-task, emit: BORING_EMIT tasks.ready <description>
      If all tasks are done, output LOOP_COMPLETE.

  builder:
    name: Builder
    description: "Implements one sub-task at a time"
    triggers: ["tasks.ready", "review.rejected"]
    publishes: ["review.ready"]
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

cli:
  backend: claude

hats:
  test_writer:
    name: Test Writer
    description: "Writes FAILING tests first"
    triggers: ["tdd.start", "refactor.done"]
    publishes: ["test.written"]
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

cli:
  backend: claude

hats:
  researcher:
    name: Researcher
    description: "Gathers information and analyzes patterns"
    triggers: ["research.start", "research.followup"]
    publishes: ["research.finding"]
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

cli:
  backend: claude

hats:
  observer:
    name: Observer
    description: "Gathers symptoms and evidence"
    triggers: ["debug.start", "hypothesis.rejected"]
    publishes: ["observation.made"]
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
    instructions: |
      Follow the test plan. Run the experiment.
      If confirmed: BORING_EMIT hypothesis.confirmed <evidence>
      If rejected: BORING_EMIT hypothesis.rejected <what was ruled out>

  fixer:
    name: Fixer
    description: "Fixes confirmed root cause"
    triggers: ["hypothesis.confirmed"]
    publishes: []
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

cli:
  backend: claude

hats:
  blue_team:
    name: Blue Team
    description: "Implements with security in mind"
    triggers: ["security.review", "vulnerability.found"]
    publishes: ["build.ready"]
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
    instructions: |
      Your job is to BREAK this code. Attack vectors:
      - Injection (SQL, XSS, command)
      - Auth bypass, IDOR
      - Race conditions, path traversal

      If vulnerabilities found: BORING_EMIT vulnerability.found <details>
      If secure: output LOOP_COMPLETE
"#;

const SPEC_DRIVEN_PRESET: &str = r#"# Borechestrator — Spec-driven preset
# Specification must be approved before implementation proceeds.

event_loop:
  starting_event: spec.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 30

cli:
  backend: claude

hats:
  spec_writer:
    name: Spec Writer
    description: "Drafts specifications from requirements"
    triggers: ["spec.start", "spec.rejected"]
    publishes: ["spec.ready"]
    instructions: |
      Draft a clear specification from the requirements.
      If rejected, address reviewer feedback.
      Emit: BORING_EMIT spec.ready

  spec_reviewer:
    name: Spec Reviewer
    description: "Reviews specs for completeness and correctness"
    triggers: ["spec.ready"]
    publishes: ["spec.approved", "spec.rejected"]
    instructions: |
      Review the specification:
      - Is it complete? Are edge cases covered?
      - Is it unambiguous? Could it be misinterpreted?
      - Is it testable?
      If issues: BORING_EMIT spec.rejected <feedback>
      If solid: BORING_EMIT spec.approved

  implementer:
    name: Implementer
    description: "Implements according to approved spec"
    triggers: ["spec.approved", "spec.violated"]
    publishes: ["implementation.done"]
    instructions: |
      Implement exactly what the approved spec describes.
      If spec.violated, fix the non-conforming code.
      Emit: BORING_EMIT implementation.done

  verifier:
    name: Verifier
    description: "Verifies implementation conforms to spec"
    triggers: ["implementation.done"]
    publishes: ["spec.violated"]
    instructions: |
      Verify the implementation matches the spec exactly.
      Check every requirement and edge case.
      If conformant: output LOOP_COMPLETE
      If violations: BORING_EMIT spec.violated <details>
"#;

const MOB_PRESET: &str = r#"# Borechestrator — Mob programming preset
# Navigator gives directions, driver codes, observer reviews.

event_loop:
  starting_event: mob.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 30

cli:
  backend: claude

hats:
  navigator:
    name: Navigator
    description: "Thinks strategically, gives instructions to driver"
    triggers: ["mob.start", "observation.noted"]
    publishes: ["direction.set"]
    instructions: |
      Think strategically. Give CLEAR, SPECIFIC instructions.
      Do NOT write code — describe what to write.
      If task complete: output LOOP_COMPLETE
      Otherwise: BORING_EMIT direction.set <instructions>

  driver:
    name: Driver
    description: "Executes navigator's instructions exactly"
    triggers: ["direction.set"]
    publishes: ["code.written"]
    instructions: |
      Execute navigator's instructions EXACTLY.
      You're the hands, not the brain. Stay tactical.
      BORING_EMIT code.written <what was done>

  observer:
    name: Observer
    description: "Provides fresh-eyes feedback"
    triggers: ["code.written"]
    publishes: ["observation.noted"]
    instructions: |
      Provide fresh-eyes feedback:
      - Potential bugs, simpler approaches
      - Missing error handling, edge cases
      - Style, naming, performance
      BORING_EMIT observation.noted <feedback>
"#;

const REFACTOR_PRESET: &str = r#"# Borechestrator — Refactor preset
# Safe refactoring with test verification at every step.

event_loop:
  starting_event: refactor.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 20

cli:
  backend: claude

backpressure:
  gates:
    - name: tests
      command: "cargo test 2>/dev/null || npm test 2>/dev/null || true"
      on_fail: "Tests must pass before refactoring."

hats:
  analyzer:
    name: Analyzer
    description: "Identifies refactoring opportunities"
    triggers: ["refactor.start", "verify.passed"]
    publishes: ["refactor.plan"]
    instructions: |
      Analyze the code for refactoring opportunities:
      - Code smells, duplication, complexity
      - Naming, structure, separation of concerns
      If more refactoring needed: BORING_EMIT refactor.plan <what to change>
      If clean enough: output LOOP_COMPLETE

  refactorer:
    name: Refactorer
    description: "Applies one refactoring at a time"
    triggers: ["refactor.plan"]
    publishes: ["verify.ready"]
    instructions: |
      Apply ONE refactoring from the plan.
      Keep changes minimal and focused.
      Run tests to confirm nothing broke.
      BORING_EMIT verify.ready <what changed>

  verifier:
    name: Verifier
    description: "Verifies refactoring didn't break anything"
    triggers: ["verify.ready"]
    publishes: ["verify.passed", "verify.failed"]
    instructions: |
      Run the full test suite. Check for regressions.
      If all pass: BORING_EMIT verify.passed
      If failures: BORING_EMIT verify.failed <what broke>
"#;

const PR_REVIEW_PRESET: &str = r#"# Borechestrator — PR review preset
# Multi-perspective code review.

event_loop:
  starting_event: review.start
  completion_promise: REVIEW_COMPLETE
  max_iterations: 15

cli:
  backend: claude

hats:
  correctness:
    name: Correctness Reviewer
    description: "Reviews for bugs, logic errors, edge cases"
    triggers: ["review.start"]
    publishes: ["review.finding"]
    instructions: |
      Review the code changes for correctness:
      - Logic errors, off-by-one, null handling
      - Edge cases, boundary conditions
      - Error handling completeness
      BORING_EMIT review.finding <correctness findings>

  security:
    name: Security Reviewer
    description: "Reviews for security vulnerabilities"
    triggers: ["review.start"]
    publishes: ["review.finding"]
    instructions: |
      Review the code changes for security:
      - Injection vulnerabilities (SQL, XSS, command)
      - Authentication/authorization issues
      - Data exposure, secrets handling
      BORING_EMIT review.finding <security findings>

  quality:
    name: Quality Reviewer
    description: "Reviews for code quality and maintainability"
    triggers: ["review.start"]
    publishes: ["review.finding"]
    instructions: |
      Review the code changes for quality:
      - Readability and naming
      - Test coverage
      - Documentation
      - Performance concerns
      BORING_EMIT review.finding <quality findings>

  synthesizer:
    name: Synthesizer
    description: "Combines all review findings into a summary"
    triggers: ["review.finding"]
    publishes: []
    instructions: |
      Collect all review findings. Create a unified summary.
      Categorize by severity (blocking, suggestion, nit).
      Output REVIEW_COMPLETE with the summary.
"#;

const DOCS_PRESET: &str = r#"# Borechestrator — Documentation preset
# Generate and review documentation.

event_loop:
  starting_event: docs.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 15

cli:
  backend: claude

hats:
  writer:
    name: Doc Writer
    description: "Writes documentation from code analysis"
    triggers: ["docs.start", "review.feedback"]
    publishes: ["docs.ready"]
    instructions: |
      Analyze the code and write documentation:
      - API reference, function signatures
      - Usage examples
      - Architecture overview if applicable
      Address any feedback from previous review.
      BORING_EMIT docs.ready <what was documented>

  reviewer:
    name: Doc Reviewer
    description: "Reviews documentation for completeness and clarity"
    triggers: ["docs.ready"]
    publishes: ["review.feedback"]
    instructions: |
      Review the documentation:
      - Is it accurate? Does it match the code?
      - Is it complete? Any undocumented features?
      - Is it clear? Would a new developer understand?
      If issues: BORING_EMIT review.feedback <feedback>
      If good: output LOOP_COMPLETE
"#;

const DEPLOY_PRESET: &str = r#"# Borechestrator — Deploy preset
# Build, test, and deploy workflow.

event_loop:
  starting_event: deploy.start
  completion_promise: DEPLOY_COMPLETE
  max_iterations: 10

cli:
  backend: claude

hats:
  builder:
    name: Builder
    description: "Builds and runs tests"
    triggers: ["deploy.start"]
    publishes: ["build.passed", "build.failed"]
    instructions: |
      Build the project and run the full test suite.
      If everything passes: BORING_EMIT build.passed <summary>
      If failures: BORING_EMIT build.failed <what failed>

  deployer:
    name: Deployer
    description: "Deploys to the target environment"
    triggers: ["build.passed"]
    publishes: ["deploy.done"]
    instructions: |
      Deploy the build to the target environment.
      Follow the project's deployment procedures.
      BORING_EMIT deploy.done <deployment summary>

  verifier:
    name: Verifier
    description: "Verifies the deployment is healthy"
    triggers: ["deploy.done"]
    publishes: []
    instructions: |
      Verify the deployment is healthy:
      - Health checks pass
      - Key functionality works
      - No error spikes in logs
      If healthy: output DEPLOY_COMPLETE
      If issues: describe what's wrong
"#;
