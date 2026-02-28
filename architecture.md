# Project architecture

## Executive summary

jev is an agent orchestrator
with two core loops:

1. **Planning loop** — an LLM generates a Rust program
   that orchestrates a workflow.
   The plan compiles or it doesn't ship.
   The planner iterates with compiler feedback
   until it produces a valid plan.

2. **Execution loop** — the compiled plan runs,
   invoking LLM subagents at runtime
   for tasks requiring fuzzy reasoning,
   while executing deterministic work
   as native compiled code.

The Rust type system and borrow checker
enforce resource access, trust levels,
and subagent capabilities at compile time.

## System architecture

### Data flow

```
Task description
    |
    v
PLANNING LOOP
    |
    |  jev CLI (plan command)
    |      |
    |      v
    |  LLM planner + API catalog
    |      |
    |      v
    |  Generated Rust source
    |      |
    |      v
    |  cargo build (rustc validates safety)
    |      |
    |      +--[compile error]--> feed back to planner
    |      |
    |      +--[success]--> compiled plan binary
    |
    v
EXECUTION (the compiled plan runs)
    |
    |  Native code: data transforms, I/O, orchestration
    |      |
    |      +---> subagent call (small model, typed)
    |      |         e.g. classify(text) -> Category
    |      |
    |      +---> subagent call (frontier model, scoped)
    |      |         e.g. summarize(&fs, &files) -> String
    |      |
    |      +---> native code: aggregate, filter, format
    |      |
    |      +---> subagent call (constrained by resources)
    |      |         e.g. generate_report(&mut out_fs, data)
    |      |
    |      v
    |  Results
```

### The planning loop

The planning loop is not a simple "call LLM once."
It's a critical, opinionated part of the system:

- The planner receives the task + API catalog
- It generates a Rust program
- `cargo build` validates it
- On compile failure, errors feed back to the planner
  for correction (currently up to 4 attempts)
- Same task + prompt inputs hash to the same plan ID,
  so repeated tasks reuse existing compiled plans

Open design questions for the planning loop:
- What permissions does the planner itself have?
- How does the human review/approve/modify plans?
- How to iterate on a plan
  (refine vs. regenerate vs. hand-edit)?
- How to maximize convenience and speed
  while maintaining safety guarantees?

### Subagent model

The compiled plan orchestrates LLM calls at runtime.
These calls are typed and resource-constrained:

**Specialized subagents** — highly typed interfaces.
A classifier might have a concrete enum return type
and use a small, cheap, fast model
(possibly running locally).
The function signature defines exactly
what goes in and what comes out.

```rust
// Hypothetical: small model, concrete types
enum Sentiment { Positive, Neutral, Negative }
let result: Sentiment = classify(&text).await;
```

**General subagents** — frontier models
with broader capabilities
but still resource-constrained.
They receive specific resources via function arguments,
and the borrow checker ensures
they can only access what's passed in.

```rust
// Frontier model, but scoped to read-only fs access
let summary = summarize(&fs, &files).await;

// Gets write access to output dir only
generate_report(&mut out_fs, &data).await;
```

**No subagent needed** — deterministic work
compiles to native code.
Data transforms, filtering, aggregation,
string manipulation, format conversion.
No LLM call, no latency, no cost.
This is work that other orchestrators
often waste LLM calls on.

### Components

**jevstd** (library crate):
Typed resource APIs and subagent interfaces.
This is the core product.
Encodes safety via Rust's type system
so the compiler enforces it.

**jev** (binary crate):
CLI that runs the planning loop
(LLM call, code generation, compilation, retry)
and executes compiled plans.

**plans/** (generated):
Each plan is a standalone Cargo project
with a path dependency on `jevstd`.

## Technology choices

**Rust** — the orchestration language.
Chosen because the type system and borrow checker
provide compile-time safety guarantees
for resource access and subagent capabilities.
Also: compiled plans run fast,
which matters when interleaving
native transforms with LLM calls.

**tokio** — async runtime.
Enables parallel subagent calls and I/O
while the borrow checker prevents
unsafe concurrent access.

**clap** — CLI argument parsing.

**reqwest** — HTTP client for LLM API calls.

**anyhow** — error handling.

**glob** — file pattern matching.

## Data architecture

### Resource access model

Resources are concrete typed objects.
Access semantics via reference types:

- `&Resource` — shared read access (parallelizable)
- `&mut Resource` — exclusive write access

The borrow checker enforces
that reads and writes never conflict.
This applies both to the plan's own operations
and to what resources subagents receive.

### Trust model

Wrapper types for trust levels:

- `Unverified<T>` — data from external sources
- `Verified<T>` — human-confirmed data
- Conversion requires explicit `.verify()` call
- Functions requiring trust take `Verified<T>`

### Plan project structure

```
plans/<id>/
├── Cargo.toml    -- path dep on jevstd, empty [workspace]
└── src/
    └── main.rs   -- LLM-generated orchestration code
```

## Implementation phases

**Phase 1 (current): Core scaffold**
- Filesystem, text, and trust types in jevstd
- CLI with plan/run/go commands
- Planning loop with compile-error retry
- Hash-based plan deduplication
- LLM exchange logging for prompt iteration

**Phase 2: Subagent primitives**
- Subagent call interface in jevstd
  (typed functions that invoke LLM reasoning)
- Model selection per subagent
  (small/local for specialized, frontier for general)
- Resource scoping via function signatures

**Phase 3: Richer resource library**
- Additional resource types
  (HTTP, email, calendar, key-value store)
- Auto-generated API catalog from crate docs

**Phase 4: Planning loop refinement**
- Plan review and approval UX
- Plan iteration (refine, regenerate, hand-edit)
- Planner permissions model
- Streaming output during planning
