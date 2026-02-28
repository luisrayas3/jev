# Project architecture

## Executive summary

jev is a two-crate Cargo workspace.
`jevstd` is the library of typed resource APIs.
`jev` is the CLI that orchestrates
plan generation, compilation, and execution.

The core architectural insight:
Rust's type system and borrow checker
replace runtime permission checks.
`&T` = read access (shared, parallelizable).
`&mut T` = write access (exclusive).
Trust levels are distinct types.
`rustc` enforces all of this at compile time.

Current phase: scaffold with filesystem,
text, and trust types.

## System architecture

### Data flow

```
Task description
    ↓
jev CLI (plan command)
    ↓
LLM (Claude API) + API catalog
    ↓
Generated Rust source (plans/<id>/src/main.rs)
    ↓
cargo build (rustc validates safety)
    ↓
Compiled binary executes
```

### Components

**jevstd** (library crate):
Typed resource APIs that encode safety
via Rust's type system.
This is the core product.

**jev** (binary crate):
CLI that calls the LLM planner,
writes generated code to plan projects,
invokes `cargo build`, and runs the result.

**plans/** (generated):
Each plan is a standalone Cargo project
with a path dependency on `jevstd`.
Plans are ephemeral build artifacts.

## Technology choices

**Rust** — the plan language and the implementation
language. Chosen because the type system and borrow
checker provide the safety guarantees
that are the core value proposition.

**tokio** — async runtime for resource operations.
Enables parallel reads via `tokio::join!`
while the borrow checker prevents
unsafe concurrent access patterns.

**clap** — CLI argument parsing.
Derive-based API for subcommand definition.

**reqwest** — HTTP client for Anthropic API calls.

**anyhow** — error handling in application code.

**glob** — file pattern matching
for `Fs::glob` implementation.

## Data architecture

### Resource access model

Resources are concrete typed objects.
Access semantics are encoded in reference types:

- `&Resource` — shared read access (parallelizable)
- `&mut Resource` — exclusive write access

The borrow checker enforces
that reads and writes never conflict.

### Trust model

Two-level trust system using wrapper types:

- `Unverified<T>` — data from external sources
- `Verified<T>` — human-confirmed data
- Conversion requires explicit `.verify()` call
- Functions requiring trust take `Verified<T>`

### Plan project structure

```
plans/<id>/
├── Cargo.toml    -- path dep on jevstd
└── src/
    └── main.rs   -- LLM-generated code
```

## Implementation phases

**Phase 1 (current): Core scaffold**
- Filesystem, text, and trust types in jevstd
- CLI with plan/run/go commands
- LLM integration via Anthropic API
- End-to-end flow working

**Phase 2: Richer resource library**
- Additional resource types
  (HTTP, email, calendar, knowledge base)
- Auto-generated API catalog from crate docs
- Compile error feedback loop

**Phase 3: Robustness**
- Plan history and management
- Streaming LLM output
- Better error messages for common failures
- Multiple LLM provider support
