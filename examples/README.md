# Examples

Real orchestration configs you can run. Start NATS first:

```bash
./scripts/dev-up.sh
```

Then run any example:

```bash
cargo run -p boring-cli -- run -c examples/<name>.yml
```

## Examples

### `self-review.yml`
Borechestrator reviews its own code. One hat reads the codebase,
another critiques it. Good first test — runs against this repo.

### `issue-to-pr.yml`
Takes a GitHub issue description (via prompt_file) and implements it.
Planner breaks it down, builder implements, reviewer validates.

### `codebase-research.yml`
Read-only research. Answers a question about a codebase without
making changes. Two hats: researcher and synthesizer.

### `tdd-kata.yml`
TDD exercise: test writer, implementer, refactorer. Give it a
coding kata via -p and watch the red-green-refactor cycle.
