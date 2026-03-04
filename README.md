# jev

Agent orchestration where the planner outputs Rust code.
`rustc` is the compiler; type errors = invalid plan.

## Project structure

```
jevs/            -- library crate (typed resource APIs)
jevs-macros/     -- proc macro crate (#[jevs::needs])
jev/             -- CLI binary
  src/main.rs    -- CLI definition + command dispatch
  src/llm.rs     -- API types, HTTP client, response parsing
  src/plan.rs    -- paths, IDs, plan lifecycle, retry loop
  src/prompt.rs  -- system prompt for the LLM
  src/exec.rs    -- build and run plan binaries
plans/           -- generated programs (gitignored targets)
tests/           -- e2e pipeline test (fish)
```

## Quick start

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [just](https://github.com/casey/just) (command runner)
- [fish](https://fishshell.com/) (for e2e tests)
- `.env` file with `ANTHROPIC_API_KEY=...` (for LLM calls)

## Development

```bash
# Build
just sync

# Plan, confirm, and run in one shot
just jev-go 'list all files in current directory'

# Or step by step:
cargo run --bin jev -- plan 'count lines in all .rs files'
cargo run --bin jev -- run

# Tests
just test         # unit + e2e
just test-unit    # unit tests only
just test-e2e     # full LLM pipeline (needs .env)

# Cleanup
just clean        # cargo clean + plan build artifacts
just clean-all    # clean + remove all plans and logs
```

## Code style

See [AGENTS.md](AGENTS.md) for complete style guide.
