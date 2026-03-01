# jev

Agent orchestration where the planner outputs Rust code.
`rustc` is the compiler — type errors = invalid plan.

## Project structure

```
jevs/                 -- library crate (typed resource APIs)
├── Cargo.toml
├── src/
│   ├── lib.rs          -- re-exports
│   ├── fs.rs           -- filesystem resource
│   ├── text.rs         -- pure text operations
│   └── trust.rs        -- trust-level types
jev/                    -- CLI binary
├── Cargo.toml
├── src/
│   └── main.rs         -- plan / run / go commands
jevu/                   -- user utility library (future)
plans/                  -- generated programs land here
```

## Quick start

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- `ANTHROPIC_API_KEY` environment variable set

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
```

## Code style

See [AGENTS.md](AGENTS.md) for complete style guide.
