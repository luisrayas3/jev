# jev

Agent orchestration where the planner outputs Rust code.
`rustc` is the compiler; type errors = invalid plan.

## Project structure

```
jevs/                 -- library crate (typed resource APIs)
├── src/
│   ├── lib.rs          -- module declarations
│   ├── api.rs          -- per-module doc aggregator
│   ├── file.rs         -- filesystem resource
│   ├── stash.rs        -- plan-local blob storage
│   ├── text.rs         -- pure text operations
│   ├── trust.rs        -- trust-level types
│   └── runtime.rs      -- RuntimeKey (init-once barrier)
jev/                    -- CLI binary
├── src/
│   └── main.rs         -- plan / run / go commands
├── assets/
│   ├── main.tmpl.rs    -- fixed shim written into plans
│   └── Cargo.tmpl.toml -- plan Cargo.toml template
plans/                  -- generated programs land here
tests/
├── e2e.fish            -- full pipeline test (fish)
└── fixtures/e2e/       -- test assets
```

## Quick start

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [just](https://github.com/casey/just) (command runner)
- [fish](https://fishshell.com/) (for e2e tests)
- `.env` file with `ANTHROPIC_API_KEY=...` (for e2e / LLM)

### Setup

```bash
cargo build
```

## Development

```bash
# Generate a plan from a task description
cargo run --bin jev -- plan 'count lines in all .rs files'

# Build and run the most recent plan
cargo run --bin jev -- run

# Plan, confirm, and run in one shot
cargo run --bin jev -- go 'list all files in current directory'

# Run tests
just test-unit    # unit tests only
just test-e2e     # full LLM pipeline (needs .env)
just test         # both
```

## Code style

See [AGENTS.md](AGENTS.md) for complete style guide.
