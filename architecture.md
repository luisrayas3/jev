# Project architecture

## Executive summary

jev is an agent orchestrator
with two core loops:

1. **Planning loop**: decomposes a task
   into a tree of subtasks,
   each declaring its resource needs.
   Dependencies resolve upward to a root
   that declares all external resources.
   The plan compiles or it doesn't ship.

2. **Execution loop**: the compiled plan runs
   in a container
   where only approved resources are mounted.
   LLM subagents handle fuzzy reasoning,
   deterministic work runs as native code.

The Rust type system and borrow checker
enforce resource access, trust levels,
and subagent capabilities at compile time.
A RuntimeKey barrier ensures
task code cannot construct resources;
only the plan's main.rs can.

## Design principles

**The type system is the security model.**
Resource access, trust levels,
and subagent capabilities are encoded as types.
If it compiles, the access patterns are valid.
No runtime permission checks,
no hoping the LLM follows instructions.
`rustc` is the safety checker.

**Resources are injected, never constructed by tasks.**
Task code receives resources as function parameters.
Resource constructors require a `RuntimeKey`,
which is initialized once at startup
with a random value tasks cannot guess.
Tasks cannot call `init` (already called)
or construct resources (no key).
This is a runtime barrier, not a convention.

**The library API is the product.**
The value is in typed resource APIs
and subagent interfaces
that make correct orchestrations natural
and unsafe ones unrepresentable.
A summarizer receives `&File` (read-only).
A report writer receives `&mut File`
scoped to an output directory.
A notification task receives `&EmailOutbox<"addr">`
but never an inbox handle;
it can send but not snoop.
An `UntrustedWeb` read returns `Unverified<T>`;
a `TrustedFile` read doesn't.
The compiler proves these constraints
before anything runs.

**Agent orchestration, not code generation.**
The output isn't "a Rust program."
It's an agent workflow
expressed as Rust,
which means the compiler can enforce properties
that other orchestrators check at runtime
or not at all.

**Compiled code for deterministic work.**
Data transforms, filtering, aggregation,
format conversion, file I/O patterns;
these don't need LLM reasoning.
They compile to native code and run fast.
Other orchestrators either waste LLM calls
on straightforward transforms
or shell out to ad-hoc scripts.
Here it's all one program.

**The planning loop is a first-class concern.**
How the plan gets iterated,
what permissions the planner has,
how the human reviews and approves,
how to maximize convenience
while maintaining safety;
this is opinionated design work,
not an afterthought.

**The permission manifest is the user contract.**
The user approves a flat list of resource grants,
not source code.
The compiler proved the code can't exceed
those grants.
The container enforces it at runtime.
This is a third option between
per-action runtime prompts
(which train users to click "yes")
and blanket access (which is unsafe).

## System architecture

### Data flow

```
Task description
    |
    v
PHASE 1: EXPAND DOWN (task decomposition)
    |
    |  Root planner decomposes task into subtasks
    |  Subtasks decompose further (tree grows down)
    |  No resource thinking yet - just what needs to happen
    |
    v
PHASE 2: IMPLEMENT LEAVES (parallelizable)
    |
    |  Each leaf gets: task desc + function signature
    |  LLM generates function body
    |  Each leaf declares its resource needs
    |
    v
PHASE 3: RESOLVE UP (mostly mechanical)
    |
    |  Child resource needs propagate to parents
    |  Shared resources pulled into parent structs
    |  Parent orchestration: join! vs sequential
    |  Root collects all external resource declarations
    |
    v
PHASE 4: COMPILE + APPROVE
    |
    |  resources.rs: root resource declarations
    |  tasks.rs: all task code (no constructor access)
    |  main.rs: fixed shim (orchestrator-generated)
    |  cargo build (rustc validates safety)
    |      |
    |      +--[compile error]--> feed back to planner
    |      |
    |      +--[success]--> permission manifest
    |                          |
    |                          v
    |                      user approves grants
    |                          |
    |                          v
    |                      container configured
    |                      from approved grants
    |
    v
EXECUTION (compiled plan in container)
    |
    |  Only approved resources mounted
    |  Native code: transforms, I/O, orchestration
    |  Subagent calls: typed, resource-scoped
    |  Results
```

### The planning loop

The planning loop has four phases,
not a single LLM call.

**Phase 1: Expand down.**
The root planner decomposes the task
into a tree of subtasks.
This is cheap and high-level:
just task descriptions, no code.
Subtasks can decompose further.

**Phase 2: Implement leaves.**
Each leaf task receives its function signature
(inputs from parent, expected outputs)
and the jevs operation API.
The LLM generates a function body.
Leaf implementations are independent
and parallelizable.

**Phase 3: Resolve up.**
Each leaf declares its resource needs.
Dependencies propagate to parents via structs.
When two siblings need the same resource,
it can't live in both child structs;
the parent keeps it in its own struct
and lends it explicitly.
The borrow checker forces the parent
to sequence conflicting access modes
or parallelize compatible ones.
This phase is largely mechanical.

**Phase 4: Compile and approve.**
The root's resource declarations
become `resources.rs`.
All task code compiles as `tasks.rs`
without access to resource constructors.
On successful compilation,
the permission manifest is derived
from `resources.rs` and presented to the user.
The user approves the grants.
The container is configured to match.

Compile errors at any point
feed back to the relevant planner
(leaf implementation or parent orchestration)
for correction.
Only the affected node replans,
not the whole tree.

### Subagent model

The compiled plan orchestrates LLM calls at runtime.
These calls are typed and resource-constrained:

**Specialized subagents**: highly typed interfaces.
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

**General subagents**: frontier models
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

**No subagent needed**: deterministic work
compiles to native code.
Data transforms, filtering, aggregation,
string manipulation, format conversion.
No LLM call, no latency, no cost.
This is work that other orchestrators
often waste LLM calls on.

**Runtime safety within the capability envelope.**
The type system constrains
what a subagent *can* access;
a summarizer that receives `&Fs`
can't send email.
Within that envelope,
subagent behavior is runtime.
When a subagent attempts an unauthorized operation
(outside its granted capabilities),
it receives an error, not silent failure.
The subagent's system prompt includes
guidance for fault recovery,
so it can adjust its approach
rather than failing opaquely.

### "Do it" mode

The plan-compile-approve flow is thorough
but adds latency for simple tasks.
"Do it" mode is an opt-in fast path
for cases where the full flow is overkill.

The user explicitly triggers "do it" mode;
it is never automatic, always a conscious choice.

- Smaller model specialized for one-liner plans
- All available resources granted upfront
- Every resource access is audited (logged)
- No compilation step, no permission approval
- Results are immediate

This trades compile-time safety guarantees
for speed and convenience on simple tasks.
The audit log provides after-the-fact visibility.
The user decides when the trade-off is appropriate.

### Components

**jevs** (library crate):
Typed resource APIs and subagent interfaces.
This is the core product.
Encodes safety via Rust's type system
so the compiler enforces it.
Exposes two layers:
- **Operations** (default): `read`, `write`, `glob`,
  `stash`, etc. Available to all task code.
- **Constructors** (`File::open`, etc.):
  Require `&RuntimeKey`.
  `RuntimeKey::init` is called once by the plan's
  main.rs with a random value;
  tasks never receive the key.
  Constructor APIs are undocumented
  in the planner prompt.
Each module has `pub const API_DOCS`
documenting its API for the planner.
`jevs::api::catalog()` aggregates all module docs.

**jev** (binary crate):
CLI that runs the planning loop
(code generation, compilation, permission extraction)
and executes compiled plans.

**jevu** (user utility crate):
Reusable local modules
containing functions developed through jev usage.
Uses jevs types and resource handles.
When the planner generates a useful function,
it can be promoted into jevu
for reuse across future plans.
Grows organically from real usage:
a personal library of proven patterns.

**plans/** (generated):
Each plan is a standalone Cargo project
with a path dependency on `jevs`
(and optionally `jevu`).

## Technology choices

**Rust**: the orchestration language.
Chosen because the type system and borrow checker
provide compile-time safety guarantees
for resource access and subagent capabilities.
Also: compiled plans run fast,
which matters when interleaving
native transforms with LLM calls.

**tokio**: async runtime.
Enables parallel subagent calls and I/O
while the borrow checker prevents
unsafe concurrent access.

**clap**: CLI argument parsing.

**reqwest**: HTTP client for LLM API calls.

**anyhow**: error handling.

**glob**: file pattern matching.

## Data architecture

### Resource access model

Resources are concrete typed objects.
Each type encodes what operations are permitted.
The borrow checker enforces
that a task actually holds a resource handle
before it can use it.

**Filesystem uses `&`/`&mut` naturally.**
`&File` is read access, `&mut File` is write access.
The borrow checker prevents concurrent read/write
conflicts automatically:

```rust
fn summarize(fs: &File) { ... }        // read
fn write_report(fs: &mut File) { ... } // write
```

**Service resources use capability-typed handles.**
For services like email, calendar, and web,
`&`/`&mut` is too coarse;
reading and sending email
are different permissions on the same service,
not shared vs exclusive access.
Instead, the type itself carries
both the capability and the trust level:

```rust
fn task(
    inbox: &TrustedEmailInbox<"luis@x.com", ["alice@y.com"]>,
    outbox: &EmailOutbox<"luis@x.com">,
) { ... }
```

`TrustedEmailInbox` has no `send` method.
`EmailOutbox` has no `read` method.
The permission boundary is the type,
not the reference mode.
The borrow checker still ensures
a task holds the handle;
a function that doesn't receive `EmailOutbox`
can't send email, period.

Trust is a property of the resource,
not just a wrapper on data.
A `TrustedEmailInbox` returns data directly.
An `UntrustedEmailInbox` returns `Unverified<T>`.
A `TrustedWeb` fetch returns usable data.
An `UntrustedWeb` fetch returns `Unverified<T>`.
The resource's trust level determines
the trust level of data flowing out of it.

This means capability handles for the same service
can be used concurrently without conflict:
reading email while sending is fine
because they're separate types.

**Resource construction requires RuntimeKey.**
Constructors like `File::open` take `&RuntimeKey`.
`RuntimeKey::init(random)` is called once
by the plan's main.rs before any task code runs.
Tasks cannot call `init` (already called, returns Err)
or guess the random key.

**Stash is plan-local content-addressed storage.**
Plans sometimes need to materialize
intermediate results to disk,
too large for memory
or shared between plan components.
`Stash` provides local working memory
with content-addressed semantics:

```rust
let stash = jevs::stash::Stash::new()?;
let handle = stash.put(&large_data).await?;  // -> Hash
// ...later, possibly in another task...
let data = stash.get(&handle).await?;
```

No naming, no paths, no conflicts;
the hash is the reference.
Tasks create stash instances directly
(no RuntimeKey required).
Stash is scoped to the plan's execution
and cleaned up on drop.
It requires no resource grant
because it's internal working memory,
not access to external resources.
Stash is always local;
network-backed storage is a resource.

### Resource scoping

Resources are scoped to specific external entities.
The scope and trust level are encoded in the type,
making both compile-time constraints.

**Filesystem**: scoped by root path,
split by trust level.
`TrustedFile<"/data">`: known-good local files,
data usable directly.
`UntrustedFile<"/uploads">`: external input,
operations return `Unverified<T>`.

**Web**: scoped by domain,
split by trust level.
`TrustedWeb<"internal.company.com">`:
known-good source, data usable directly.
`UntrustedWeb<"reddit.com">`:
operations return `Unverified<T>`.
Domain-level scoping balances specificity
(auditable grants)
with practicality (URLs are dynamic).

**Email**: scoped by account and filtered by sender.
`TrustedEmailInbox<"luis@x.com", ["alice@y.com"]>`:
reads that inbox, filtered to trusted senders,
data usable directly.
`UntrustedEmailInbox`: external sources,
returns `Unverified<T>`.
`EmailOutbox<"luis@x.com">`: sends from that address;
recipient must be a `TrustedRecipient`.
Backed by a contact book
with per-contact settings and roles,
enabling template-driven composition
(e.g., a "colleague" role
with appropriate tone and signature).

**Sockets**: local paths as IPC endpoints.
A socket resource is like a scoped `Fs`
for inter-process communication,
subject to the same permission grants.

These scoping patterns make the permission manifest
specific and auditable:
the user sees exactly which domains,
which email addresses,
and which filesystem paths the plan touches.

### Resource struct propagation

Each node in the task tree has its own struct
for the resources it needs.
By default, child structs nest in the parent:

```rust
struct RootResources {
    summarize: SummarizeResources,
    format: FormatResources,
}
```

When siblings share a resource,
it can't be in both child structs
(two owners of the same value).
It gets pulled into the parent struct
and lent to children explicitly:

```rust
struct RootResources {
    data: File,  // shared: both children need it
    summarize: SummarizeResources,
    format: FormatResources,
}

// parent lends &data to both (parallel ok)
let (a, b) = tokio::join!(
    summarize(&res.data, &res.summarize),
    format(&res.data, &res.format),
);
```

This propagation is largely mechanical.
Only access-mode conflicts (read vs write
on the same resource) require
the parent to decide ordering.

### Trust model

Trust operates at two levels:

**Resource-level trust** determines
what comes out of a resource.
`TrustedWeb` reads return data directly.
`UntrustedWeb` reads return `Unverified<T>`.
The resource's trust level is set at construction
based on user configuration
(which domains, senders, paths are trusted).

**Data-level trust** wraps values:
- `Unverified<T>`: data from untrusted resources
- `Verified<T>`: human-confirmed data
- `.verify()` triggers real human confirmation
  (not a no-op cast)
- Functions requiring trust take `Verified<T>`;
  passing `Unverified<T>` is a compile error

The two levels connect:
trusted resources produce usable data,
untrusted resources produce `Unverified<T>`,
and sensitive operations (like `EmailOutbox::send`)
require `TrustedRecipient`,
verified via the contact book,
not raw addresses.

### Sandboxing

Plans run in a container.
The container configuration is derived directly
from the approved permission manifest:
each granted resource maps to a mount or network rule.

- Filesystem grants → mounted paths (read-only or read-write)
- Network grants → allowed endpoints
- No grant → not mounted, not reachable

`std::fs`, `std::net`, `std::process` etc.
are not useful inside the container
because nothing is mounted
beyond what the grants specify.
Combined with the RuntimeKey barrier
(task code can't construct resources),
this closes the escape hatch.

### Permission manifest

The manifest is derived from `resources.rs`
and is the user-facing audit surface:

```
This plan requires:
  TrustedFile    read   /data/**              (summarize, format)
  TrustedFile    write  /output/report        (root)
  UntrustedWeb   fetch  news.ycombinator.com  (scrape)
  TrustedInbox   read   luis@x.com [alice]    (scan-inbox)
  EmailOutbox    send   luis@x.com → alice    (notify)
```

Each entry lists the access mode,
the resource path,
and which tasks use it.
Conditional accesses (behind `if` branches)
can be annotated.

The user approves this list,
not source code.

### Plan project structure

```
plans/<id>/
├── Cargo.toml
└── src/
    ├── main.rs        -- fixed shim (embedded asset)
    ├── resources.rs   -- resource declarations (audited)
    └── tasks.rs       -- task implementations (LLM-generated)
```

`main.rs` is a fixed embedded asset, not LLM-generated.
It initializes a random RuntimeKey,
calls `resources::create(&key)`,
then passes the result to `tasks::root()`.
This is the same for every plan.

`resources.rs` is the auditable dispatch:
a struct declaring which resources the plan uses
and a `create` function that constructs them.
This is what the permission manifest is derived from.

## Implementation phases

Phases are ordered to maximize feedback
from real personal-assistant usage.
The safety foundation (Phase 2) lands first
so every subsequent resource is safe from day one.
Resources (Phase 3) come before task trees (Phase 4)
because without real-world resources
the system isn't useful,
and without the safety layer
it isn't trustworthy.

**Phase 1 (done): Core scaffold**
- Filesystem, text, and trust types in jevs
- CLI with plan/run/go commands
- Single-shot planning with compile-error retry
- Single `main.rs` per plan (flat, no task tree)
- LLM exchange logging for prompt iteration

**Phase 2 (done): Safety foundation**
- RuntimeKey barrier: `init(random)` once at startup,
  `File::open(&key, root)` requires the key,
  tasks never receive it
- Split plan into `resources.rs` + `tasks.rs`
- Fixed orchestrator-generated `main.rs` (embedded asset)
- `resources.rs` as auditable dispatch
  (struct + `create(&key)`)
- Permission manifest UX
  (structured resource display, not raw code)
- Stash: plan-local blob storage,
  created by tasks directly (no key required)
- Per-module `API_DOCS` + `jevs::api::catalog()`
- Qualified imports (no `use jevs::*`)

**Phase 3: Real-world resources**
- Trust-level resource types
  (`TrustedWeb`/`UntrustedWeb`,
  `TrustedEmailInbox`/`UntrustedEmailInbox`,
  `EmailOutbox`, `TrustedFile`/`UntrustedFile`)
- Opinionated web resource
  (`Web::fetch(url) -> Document`,
  `Api::get`, `Api::post`)
- Email resource with contact book integration
  (per-contact settings, roles for templating;
  send requires `TrustedRecipient`)
- Calendar resource (read, create, modify)
- jevu: user utility library
  (promote reusable functions from prior plans)
- Saved plans + rerunning
  (named plans, `jev run <name>`,
  parameterized templates)
- Each new resource is safe from day one
  via the compilation boundary

**Phase 4: Task trees + orchestration**
- Expand-down / resolve-up planning loop
- Per-node resource structs with propagation
- Parallel leaf implementation
- Mechanical resource hoisting for shared access
- Typed/structured subagent interfaces
- Model selection per subagent

**Phase 5: Sandboxing + hardening**
- Containerized plan execution
- Container config derived from permission manifest
- Conditional access annotations
- Trust type `.verify()` as real human confirmation
- "Do it" mode
  (opt-in fast path, audit-logged,
  no compilation)
