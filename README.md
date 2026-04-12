# borechestrator

The world's most boring AI agent orchestrator.

*Your agents scale until AWS and Anthropic both send you emails.*

**You probably don't need this.** If your agents fit on one machine, use [Ralph](https://github.com/mikeyobrien/ralph-orchestrator). It's simpler and it's what we based this on. Borechestrator is for when you've outgrown a single box and need the agents to run as K8s Jobs, share state through S3, and coordinate through a message broker. If that sentence didn't make you nod, close this tab.

## Why?

Because every AI agent orchestrator is trying to be clever, and we're tired of it.

An AI agent orchestrator is just the AI agent loop, but with the ability to call other agents as tools. That's it. That's the whole idea. You don't need a novel framework. You don't need a new paradigm. You don't need seventeen abstractions over "send a message and wait for a response."

You know what you need? A message broker, an object store, and a job scheduler. Your platform team has been running these for decades. They're boring. They work.

Borechestrator takes the conceptual model from [Ralph orchestrator](https://github.com/mikeyobrien/ralph-orchestrator) — which is damn near perfect for local agent orchestration — and makes it work across machines. No magic. Just the boring cloud-native stuff your IT department already knows how to operate:

- **K8s Jobs** for agent execution (scheduling, retries, resource limits, RBAC — all free)
- **NATS JetStream** for events (wildcard routing, persistence, exactly-once delivery)
- **S3-compatible storage** for scratchpads (versioned, IAM-controlled, boring)
- **K8s Secrets / env vars / AWS Secrets Manager** for credentials (pick your boring)

Your security team already knows how to audit this. Your platform team already knows how to monitor this. Your oncall already knows how to debug this. Nobody needs to learn a new permission model, a new observability stack, or a new deployment pipeline.

It's just K8s. It's just NATS. It's just S3. It's just boring.

## Architecture

### The Big Picture

```mermaid
graph TB
    Config[borechestrator.yml] --> Controller[Boring Controller]
    Controller -->|subscribe| NATS[NATS JetStream]
    Controller -->|create jobs| K8s[K8s Jobs]
    Controller -->|read/write| S3[S3-Compatible Store]
    Controller -->|resolve| Secrets[Secret Provider]

    K8s -->|agent stdout| Controller
    K8s -->|publish events| NATS

    subgraph "Per Hat Activation"
        Job[K8s Job / Local Process]
        Agent[boring-agent]
        CLI[AI CLI Tool]
        Job --> Agent --> CLI
    end

    K8s --> Job
```

### Event Flow

Hats are specialized agent personas. Each hat subscribes to event topics and publishes new events when done. The controller routes events to hats based on trigger patterns. That's the whole orchestration model.

```mermaid
sequenceDiagram
    participant C as Controller
    participant N as NATS
    participant P as Planner Hat
    participant B as Builder Hat
    participant V as Verifier Hat

    C->>N: publish(pipeline.start)
    N->>C: event: pipeline.start
    C->>P: create job
    P->>P: echo "BORING_EMIT build.ready ..."
    P-->>C: stdout parsed
    C->>N: publish(build.ready)
    N->>C: event: build.ready
    C->>B: create job
    B->>B: echo "BORING_EMIT verify.ready ..."
    B-->>C: stdout parsed
    C->>N: publish(verify.ready)
    N->>C: event: verify.ready
    C->>V: create job
    V->>V: echo "LOOP_COMPLETE"
    V-->>C: stdout parsed
    C->>N: publish(_system.completion)
    Note over C: Run complete
```

### Crate Structure

```mermaid
graph LR
    Proto[boring-proto] --> Broker[boring-broker]
    Proto --> Store[boring-store]
    Proto --> Controller[boring-controller]
    Proto --> Secrets[boring-secrets]
    Broker --> Controller
    Store --> Controller
    Runtime[boring-runtime] --> Controller
    Secrets --> Controller
    Controller --> CLI[boring-cli]
    Proto --> Agent[boring-agent]
```

| Crate | What it does |
|---|---|
| `boring-proto` | Types: Topic, Event, Config. Zero heavy deps. |
| `boring-broker` | Broker trait + NATS JetStream implementation |
| `boring-store` | Store trait + local filesystem (S3 coming) |
| `boring-runtime` | Runtime trait + local process (K8s coming) |
| `boring-secrets` | SecretProvider trait + env, file, K8s, AWS impls |
| `boring-controller` | Event router, job builder, reconciler loop |
| `boring-cli` | The `boring` CLI binary |
| `boring-agent` | Runs inside containers (coming soon) |

### Secret Resolution

```mermaid
graph LR
    Config[hat env config] --> Resolver[JobBuilder.resolve_env]
    Resolver --> Chain[ChainSecretProvider]
    Chain --> Env[BORING_SECRET_* env vars]
    Chain --> File[~/.boring/secrets/ files]
    Chain --> K8s[K8s Secrets]
    Chain --> AWS[AWS Secrets Manager]
    Resolver --> Job[JobSpec with resolved env]

    style Env fill:#90EE90
    style File fill:#90EE90
    style K8s fill:#FFE4B5
    style AWS fill:#FFE4B5
```

Green = implemented. Orange = feature-gated, needs `--features k8s` or `--features aws`.

## Config

Borechestrator uses a YAML config that's a superset of Ralph's format. If you know Ralph, you know this.

```yaml
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 50

hats:
  planner:
    name: Planner
    description: "Breaks work into sub-tasks"
    triggers: ["work.start", "subtask.done"]
    publishes: ["subtask.ready"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Read the task. Break it into sub-tasks.
      Emit BORING_EMIT subtask.ready for each one.
    env:
      ANTHROPIC_API_KEY:
        from_secret: anthropic-api-key

  builder:
    name: Builder
    description: "Implements a sub-task"
    triggers: ["subtask.ready"]
    publishes: ["subtask.done"]
    command: "claude --print \"$BORING_PROMPT\""
    instructions: |
      Implement the sub-task described in the event payload.
      When done, emit BORING_EMIT subtask.done.
    env:
      ANTHROPIC_API_KEY:
        from_secret: anthropic-api-key
```

### Event Convention

Agents emit events by printing to stdout:

```
BORING_EMIT <topic> <payload>
```

The completion promise (e.g., `LOOP_COMPLETE`) anywhere in stdout signals the run is done.

### Topic Wildcards

NATS-compatible. Because why invent a new pattern matching syntax.

- `work.start` — exact match
- `work.*` — matches `work.start`, `work.done`, not `work.sub.deep`
- `work.>` — matches `work.start`, `work.sub.deep`, everything under `work.`
- `>` — matches everything

## Quick Start

### Local Dev

```bash
# Start NATS + RustFS
./scripts/dev-up.sh

# Validate a config
cargo run -p boring-cli -- validate -c my-config.yml

# Run an orchestration
cargo run -p boring-cli -- run -c my-config.yml

# Tear down
./scripts/dev-down.sh
```

### Kubernetes

```bash
# Deploy NATS + RustFS via Helm
./scripts/k8s-up.sh

# Tear down
./scripts/k8s-down.sh
```

### Secrets

Set secrets as env vars:

```bash
export BORING_SECRET_ANTHROPIC_API_KEY=sk-ant-...
```

Or drop them in files:

```bash
echo "sk-ant-..." > ~/.boring/secrets/anthropic-api-key
```

Then reference them in your config:

```yaml
env:
  ANTHROPIC_API_KEY:
    from_secret: anthropic-api-key
```

## Building Agent Images

Borechestrator doesn't care what's in the container. Its job is orchestration — routing events, scheduling jobs, managing state. What AI tool you use, what MCP servers you run, what skills you install — that's your Dockerfile.

The base image has `boring-agent`, which handles the plumbing (S3 sync, NATS events, git). You add the AI tool on top:

### Claude Code

```dockerfile
FROM borechestrator/agent:latest

# Install Claude Code
RUN curl -fsSL https://claude.ai/install.sh | sh

# Your MCP servers, skills, CLAUDE.md, whatever
COPY .claude/ /home/agent/.claude/
COPY CLAUDE.md /workspace/CLAUDE.md
```

### Claude Code + MCP Servers

```dockerfile
FROM borechestrator/agent:latest

RUN curl -fsSL https://claude.ai/install.sh | sh
RUN npm install -g @modelcontextprotocol/server-github
RUN npm install -g @anthropic/mcp-server-postgres

COPY .claude/ /home/agent/.claude/
```

### Codex (for review hats)

```dockerfile
FROM borechestrator/agent:latest

RUN npm install -g @openai/codex
```

### Custom Python Agent

```dockerfile
FROM borechestrator/agent:latest

RUN pip install anthropic
COPY my_agent.py /usr/local/bin/my-agent
RUN chmod +x /usr/local/bin/my-agent
```

### Mixed Models in One Orchestration

The whole point of hats is that each one can be a different tool:

```yaml
cli:
  backend: claude  # default for hats without a command

hats:
  planner:
    name: Planner
    # Uses default backend (claude)
    instructions: |
      Break the work into tasks...

  builder:
    name: Builder
    image: ghcr.io/myorg/claude-with-mcp:latest
    # Custom image with MCP servers for database access
    instructions: |
      Implement the feature...

  reviewer:
    name: Reviewer
    image: ghcr.io/myorg/codex-reviewer:latest
    command: "codex \"$BORING_PROMPT\""
    # Different model, different strengths
    instructions: |
      Review for correctness and edge cases...
```

### The `.boring/` Directory

Every agent container gets a `.boring/` directory materialized from S3 before the command runs. The AI CLI can grep and read it like any other file:

```
.boring/
  prompt.md              # the assembled prompt
  event.json             # current event context
  scratchpad/            # all hat scratchpads (cross-hat visible)
    planner.md
    builder.md
    shared.md
  memories.md            # human-readable learned patterns
  memories.json          # raw JSON for programmatic access
  tasks.md               # task checklist
  tasks.json             # raw JSON
```

Your AI tool doesn't need a special plugin or API to access orchestration state. It's just files. `grep -r "pattern" .boring/` works. `cat .boring/memories.md` works. Whatever your tool does with files, it does with `.boring/`.

After the command finishes, `boring-agent` syncs modified files back to S3 and commits them to git. Other hats in the same run can see your scratchpad updates on their next activation.

### What Borechestrator Is Not

Borechestrator is not a framework for building AI agents. It doesn't wrap the Anthropic API. It doesn't manage conversation history. It doesn't handle tool use or function calling. There are plenty of frameworks for that, and they're all trying to be clever.

Borechestrator is the boring glue between agents that already exist. You bring the agent. We bring the plumbing.

## Status

This is early. The local-mode pipeline works end-to-end. Here's what exists and what doesn't:

- [x] YAML config parsing (Ralph-compatible superset)
- [x] NATS-compatible topic wildcard matching
- [x] Event routing with specificity priority
- [x] Local process runtime
- [x] Local filesystem store
- [x] Reconciler loop (the actual orchestrator)
- [x] Stdout event parsing (`BORING_EMIT`)
- [x] Secret management (env, file, K8s, AWS)
- [x] CLI: `validate`, `run`, `emit`
- [x] End-to-end tests against live NATS
- [x] K8s Job runtime (kube-rs, tested against Docker Desktop K8s)
- [x] Docker runtime (docker run, tested)
- [x] S3 store implementation (aws-sdk-s3, tested against RustFS)
- [x] `boring-agent` container wrapper (with Dockerfile)
- [x] Git integration (clone, branch, push per job)
- [x] Backpressure gates (global + per-hat quality checks)
- [x] Concurrent hat execution (parallel job creation)
- [x] Metrics (atomic counters, JSON snapshots)
- [x] `boring init` scaffolding (12 presets)
- [x] Multi-backend abstraction (`cli.backend`)
- [x] prompt_file support + CLI `-p` / `-P` flags
- [x] Memories system (pattern, decision, fix, context)
- [x] Tasks system (work item tracking)
- [x] Hooks system (pre/post lifecycle events)
- [x] wave_id event grouping
- [x] required_events fan-in barrier
- [x] Run checkpointing
- [x] Human-in-the-loop (CLI + extensible trait)
- [x] `.boring/` workspace materialization (grep-friendly state)
- [x] `boring status` / `boring logs` commands
- [x] OTel tracing bridge (feature-gated, OTLP export)
- [x] Helm chart for borechestrator controller
- [x] Docker images + GitHub Actions CI/CD

## Name

It's called borechestrator because it's boring. That's the point. If your orchestrator is exciting, something has gone wrong.

## License

MIT. Inspired by [Ralph orchestrator](https://github.com/mikeyobrien/ralph-orchestrator) (also MIT).
