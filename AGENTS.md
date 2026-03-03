# Guidance for AI agents

This file guides AI agents working in this repository.
It establishes writing conventions,
documentation structure,
and provides the essential context needed
for AI agents to operate in the repo.

## Project overview

jev is an agent orchestration system
where the planner outputs Rust code.
Libraries represent real resources
(filesystem, email, calendar, knowledge base)
with typed APIs
that encode safety semantics via Rust's type system.

Target users are developers building AI agent workflows.
The core insight is that `rustc` is the safety checker:
type errors = invalid plan,
the borrow checker enforces grant safety,
and information flow (confidentiality, integrity)
is encoded in the type system.
No custom IR, no interpreter; Rust is the whole story.

Key architectural decisions:
- Task code receives resources as parameters,
  never constructs them (compilation boundary)
- Plans decompose into task trees;
  resources resolve upward to a single root
- The user approves a permission manifest,
  not source code
- Plans run in containers
  where only approved resources are mounted

This project is in early development / concepting phase.
Schemas and APIs can break freely.

## Documentation structure

This repository maintains focused,
single-purpose documentation files:

**AGENTS.md** (this file):
Writing conventions and agent-specific guidance.
Wraps other documentation with agent-specific context.

**README.md**:
Human-oriented quick start:
- Setup instructions
- Project structure (directory layout)
- Development commands (e.g. API endpoints or CLI usage)
- Pointers to key subcomponent documentation

**vision.md**:
Product vision and philosophy:
- Core principles
- What makes this different
- Long-term goals

**architecture.md**:
Technical design decisions:
- System architecture
- Technology choices and rationale
- Core schemas
- Implementation phases

Keep documentation DRY: each file has one purpose.
Link between files rather than duplicating content.

## Documentation style guide

All documentation files in this repository
follow these conventions for consistency and readability.

### Semantic line breaks (SemBr)

Use semantic line breaks to structure prose.
Break lines at natural thought boundaries:
- End of sentences
- Before/after nested clauses that add complexity
- At logical topic shifts within a paragraph
- When a phrase completes a distinct idea

Do not break mechanically at every comma or conjunction.
Short lists and simple phrases can remain on one line.
The goal is readability and clean diffs,
not rigid punctuation rules.

### Line length

Keep lines under 72 characters maximum
(easy with SemBr),
except for URLs and code blocks
which can exceed this limit.

### Header formatting

Use sentence casing for all headers, titles, and labels.
Only capitalize the first word and proper nouns.

Examples:
- "Documentation style guide" (correct)
- "Documentation Style Guide" (incorrect)

### Punctuation

Never use em dashes.
Use commas, semicolons, colons, or parentheses instead.

### Writing style

Keep phrasing concise and straightforward.
Prefer direct statements over verbose explanations.
Remove unnecessary words and filler.

Focus on clarity and actionability:
write so readers can quickly understand
and act on the information.

Avoid superlatives and marketing language.
State facts, not opinions.

## Code style conventions

### General principles

**DRY (Don't Repeat Yourself)**:
Avoid duplication across code and documentation.
Reference existing patterns rather than recreating them.

**KISS (Keep It Simple, Stupid)**:
Only create abstractions
when you have three or more instances.
Prefer simple, direct code over premature generalization.

**Consistent naming**:
Follow existing patterns in the codebase.
Use the same terms for the same concepts throughout.

**Readable code, minimal comments**:
Prefer tight, elegant, _readable_ code
over lots of code comments to explain what's happening.
Comments are useful to explain "why"
or provide non-obvious context,
and only sparingly to clarify what a block of code does.

### Coding conventions

**Camel case definition** (applies to all languages):
Treat acronyms and initialisms as words in identifiers.
- `HttpClient` not `HTTPClient`
- `costUsd` not `costUSD`
- `ApiKey` not `APIKey`

#### Rust

- Follow standard Rust naming: `snake_case` for functions
  and variables, `PascalCase` for types and traits
- Use `anyhow::Result` for error handling in binaries
  and application code
- Prefer `&str` over `String` in function parameters
- Follow camel case definition: `HttpClient`, `ApiKey`
- Keep `use` statements grouped: std, external, internal
- `jevs` public API must never panic;
  always return `Result` or `Option`
  so plans are forced to handle errors

#### Fish

- Use 2-space indentation
- Use lowercase for locals, prefer locals to globals

## Memorized commands

**"Please review the docs"** = Read all /*.md files
for complete project context

**"Please update the docs"** = Update all /*.md files
with decisions and changes from this session

## Agent workflow

**Before modifying code:**
- Read existing files to understand current patterns
- Follow established conventions strictly
- Reference architecture.md for design decisions

**When to ask questions:**
- Multiple valid approaches exist
- Requirements are ambiguous
- Major architectural decisions needed
- Trade-offs require human judgment

**After significant changes:**
- Update AGENTS.md with new status and key file locations
- Revise README.md if setup changed
- Modify architecture.md if design evolved

### Important reminders

- Do what has been asked; nothing more, nothing less
- Prefer working with existing files & code
  vs creating new ones
- Only create documentation if explicitly requested
- Follow the style guide strictly
- Reference existing patterns rather than recreating
- Document current state, avoid commenting diffs
- Keep code changes and comments tight and focused

## Miscellaneous guidance

- Generated plan projects live in `plans/` and are
  gitignored (`plans/*/target`); the plans themselves
  are ephemeral
- The `jevs` API surface is the core product;
  changes there should be deliberate and well-considered
- Each jevs module has `pub const API_DOCS`
  documenting its API; `jevs::api::catalog()`
  aggregates them for the planner prompt
- The planner prompt in `jev/src/main.rs` must stay
  in sync with the actual `jevs` public API
- Constructor APIs (`File::open`, `FileTree::open`,
  `RuntimeKey::init`) are deliberately undocumented
  in the planner prompt; tasks should never call them
- Plan assets live in `jev/assets/`:
  `main.tmpl.rs` (fixed shim),
  `Cargo.tmpl.toml` (template with
  `{plan_name}`, `{jevs_path}` placeholders)
- E2e test uses fish and requires `.env`
  with `ANTHROPIC_API_KEY`

## Current status

**Phase**: 3 in progress
(see architecture.md for phased roadmap)

**What's implemented**:
- `jevs` library: `file`, `stash`, `text`, `label`,
  `gate`, `runtime` modules with per-module API docs
- `jevs::label::Labeled<T, C, I>`:
  two-axis IFC wrapper carrying confidentiality
  (Private/Public) and integrity (Me/Friend/World);
  `map`, `join` (lattice join), `inner`, `into_inner`,
  `Labeled::local(value)` (Public, Me)
- `jevs::declassify!` / `jevs::accredit!` macros:
  each call site registers a `CrossingInfo` static
  via `linkme` distributed slice;
  `gate::init()` iterates all crossings at startup,
  collects per-crossing decisions (allow/prompt/reject);
  empty slice = auto-approve (no crossings, runs immediately)
- `declassify_gated` / `accredit_gated` methods
  on `Labeled`, called by the macros;
  direct `.declassify()` / `.accredit()` remain as stubs
  (existing tests unchanged)
- `jevs::gate` module: `CrossingInfo`, `Policy`,
  `Decision`, `init()`, `check()`,
  test injection (`inject_decision`, `inject_response`)
- `Classification` and `Integrity` traits
  with `name() -> &'static str`
- `Declassifiable` trait for bounded-output types
- `SatisfiesClassification` / `SatisfiesIntegrity` trait bounds
- `jevs::file::File<C, I>` and
  `jevs::file::FileTree<C, I>`:
  resources carry labels as type parameters;
  `read()` returns `Labeled<String, C, I>`;
  `write()` requires compatible labels
  via `SatisfiesClassification` + `SatisfiesIntegrity` bounds;
  trailing `/` in URL distinguishes them
- `jevs::api::catalog()` aggregates module docs
- `jev` CLI with `plan`, `run`, and `go` subcommands
- LLM integration via Anthropic API
- LLM outputs two fenced blocks:
  ```rust``` (tasks.rs) + ```toml``` (resource decls)
- Resource declarations: TOML with URL + access +
  optional confidentiality/integrity labels
  (defaults: private/me);
  auto-generates `resources.rs`
  (struct with label type params + `create(&key)`)
- URL-based resource identification
  (`file:./` = directory, `file:./foo` = file)
- RuntimeKey barrier
- Permission manifest with labels
- Plan structure: `main.rs` (embedded asset) +
  `resources.rs` (auto-generated from decls) +
  `tasks.rs` (LLM)
- Plan `main.rs` calls `jevs::gate::init()?`
  before `RuntimeKey::init`
- Plan `Cargo.toml` includes `linkme` dependency
  (macros reference `::linkme::` in plan crate)
- Compile error feedback loop
  (retry with error context, up to 4 attempts)
- Unit tests (46) + e2e test (fish, full pipeline)

**What's NOT implemented yet**:
- Human confirmation for `declassify`/`accredit`
  is wired but the runtime prompt gate
  (`Policy::Prompt` path) is interactive stdin only;
  no TUI or structured approval UX yet
- Principal tiers with contact book
  (contacts map to integrity tiers)
- Sandbox as capability type
  (mounts, derived labels, shell access,
  taint-capability check at construction)
- Vanilla subagents (LLM in sandbox)
- Jev planner subagents
  (nested planning loop,
  compilation as integrity endorsement)
- Additional resource types
  (email, calendar, knowledge base, HTTP)
- Task tree decomposition
  (expand down / resolve up planning loop)
- Containerized execution
- jevu user utility library
  (reusable functions from prior plans)
