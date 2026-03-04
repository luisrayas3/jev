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
enforce resource access,
information flow (confidentiality and integrity),
and subagent capabilities at compile time.
A RuntimeKey barrier ensures
task code cannot construct resources;
only the plan's main.rs can.

## Design principles

**The type system is the security model.**
Two axes from information flow control:
confidentiality (who can see data)
and integrity (who has endorsed data).
Both are encoded as types.
Resource access, information flow labels,
and subagent capabilities
are all compile-time constraints.
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
A notification task receives `&EmailOutbox`
but never an inbox handle;
it can send but not snoop.
Data from low-integrity resources
is labeled and cannot flow
into high-integrity actions.
Private data cannot flow
to world-visible outputs.
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

**Compiler-verified plans auto-approve.**
If a compiled plan has no declassifications
or accreditations,
it is safe by construction and runs immediately.
Label crossings require human confirmation.
The permission manifest is always available
as an audit artifact but is not a blocking gate.

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
PHASE 4: COMPILE + VERIFY
    |
    |  tasks.rs: all task code with #[jevs::needs]
    |  main.rs: fixed shim (orchestrator-generated)
    |  cargo build (rustc validates safety)
    |      |
    |      +--[compile error]--> feed back to planner
    |      |
    |      +--[success]--> permission manifest
    |                          |
    |            [no label crossings]--> auto-approve
    |            [label crossings]----> human confirms
    |                               boundary crossings
    |                          |
    |                          v
    |                      container configured
    |                      from declared resources
    |
    v
EXECUTION (compiled plan in container)
    |
    |  Only declared resources mounted
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

**Phase 4: Compile and verify.**
The root's resource declarations
become `resources.rs`.
All task code compiles as `tasks.rs`
without access to resource constructors.
On successful compilation,
plans with no label crossings auto-approve;
plans with label crossings require human confirmation
of the specific boundary crossings.
The container is configured
from the declared resources.

Compile errors at any point
feed back to the relevant planner
(leaf implementation or parent orchestration)
for correction.
Only the affected node replans,
not the whole tree.

### Subagent model

Runtime subagents are LLM calls
that the compiled plan invokes during execution.
All runtime subagents execute inside sandboxes
(see [sandboxing](#sandboxing)).
There are two kinds,
plus deterministic work that needs no LLM.

**Vanilla subagents** run an LLM in a sandbox.
The LLM can use tools exposed by mounted resources
and optionally run shell commands.
Everything in the sandbox's context
(data the LLM sees, resources it can call)
is subject to information flow rules.
The sandbox's labels are the lattice join
of all inputs: most restrictive confidentiality,
least trustworthy integrity.

```rust
// Classifier: small model, bounded output type.
// Sentiment is Declassifiable,
// so output is clean despite tainted input.
let sentiment: Sentiment = sandbox.call(
    model::fast, classify_prompt,
).await?;

// Summarizer: frontier model, string output.
// String is not Declassifiable,
// so output carries the sandbox's labels.
let summary: Labeled<String> = sandbox.call(
    model::frontier, summarize_prompt,
).await?;
```

The critical safety rule:
if any input to the sandbox has low integrity,
the sandbox cannot also hold
action-capable resources.
An action resource's methods require
a minimum integrity level on their inputs;
the sandbox's integrity (derived from mounts)
must satisfy that requirement
or construction is a compile error.
The container enforces this at runtime too:
unmounted resources are unreachable,
even via shell.

**Jev planner subagents** run a nested jev
planning loop instead of a direct LLM call.
The planner receives:

1. *Planning context*: task description
   and resource type signatures.
   Subject to normal information flow rules.
2. *Resource handles*: the generated plan
   will wire these together,
   but the planner does not read their data.
   These do not taint the planning context.

The planner generates Rust code
that `rustc` compiles,
proving the information flow between resources
is safe.
This means a jev planner subagent
can orchestrate resources
that would be incompatible
in a single vanilla subagent context,
because the compiled code provably separates them:

```rust
// A vanilla subagent CANNOT hold both
// untrusted_inbox and outbox
// (World integrity < outbox's requirement).
//
// A jev planner subagent CAN receive both
// as resource handles and generate a plan
// that reads untrusted emails,
// extracts Declassifiable values (count, enum),
// and routes only those to the outbox path.
// rustc proves the separation.
```

Compilation is an integrity endorsement mechanism:
the compiler proves the generated code respects
information flow constraints.
No additional user approval is needed
unless the sub-plan requires a new resource grant
(beyond what the parent already approved)
or needs to cross label boundaries
(declassify or accredit beyond automatic rules).

**Deterministic work** needs no subagent.
Data transforms, filtering, aggregation,
string manipulation, format conversion.
These compile to native code and run fast.
Other orchestrators often waste LLM calls
on straightforward transforms.

### Auto-approve

Each `jevs::declassify!` or `jevs::accredit!` call
registers a `CrossingInfo` static
in a `linkme` distributed slice.
At plan startup, `gate::init()` iterates
all registered crossings.

A compiled plan with no crossings
(empty distributed slice)
is safe by construction:
the type system proved
no untrusted data reaches action boundaries
and no private data leaks to public outputs.
`gate::init()` returns immediately
and the plan runs without prompts.

Plans with crossings show each at startup.
The user decides per crossing:
allow (pass silently at runtime),
prompt (ask again when reached), or reject.
The binary is self-describing;
the orchestrator does not need to know
about crossings.

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
it can be curated into jevu
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

Resources are concrete typed objects
carrying three compile-time properties:

1. **Access mode**: `&` (read) vs `&mut` (write),
   enforced by the borrow checker
2. **Confidentiality**: who can see data
   flowing out of this resource
3. **Integrity**: who has endorsed this data
   (which principal tier the resource belongs to)

Access mode is orthogonal
to confidentiality and integrity.
`&`/`&mut` controls read vs write;
confidentiality/integrity control
where data can flow and what it can influence.

**Filesystem uses `&`/`&mut` naturally.**
Two types: `File` (single file)
and `FileTree` (directory).
`&File` / `&FileTree` is read access,
`&mut File` / `&mut FileTree` is write access.
The borrow checker prevents concurrent read/write
conflicts automatically:

```rust
fn read_config(cfg: &File) { ... }         // read
fn write_report(fs: &mut FileTree) { ... }  // write
```

**Service resources use capability-typed handles.**
For services like email, calendar, and web,
`&`/`&mut` is too coarse;
reading and sending email
are different permissions on the same service,
not shared vs exclusive access.
The type itself carries the capability:

```rust
fn task(
    inbox: &EmailInbox<"luis@x.com">,
    outbox: &EmailOutbox<"luis@x.com">,
) { ... }
```

`EmailInbox` has no `send` method.
`EmailOutbox` has no `read` method.
The permission boundary is the type,
not the reference mode.
Capability handles for the same service
can be used concurrently without conflict.

**Resource labels determine data labels.**
A resource's confidentiality and integrity
flow into the data it produces.
Data from a high-integrity resource
(e.g., an inbox filtered to Friend-tier contacts)
carries `Friend` integrity.
Data from a low-integrity resource
(e.g., an unfiltered web fetch)
carries `World` integrity.
Data from a private resource carries `Private`
confidentiality; from a public resource, `Public`.
The data wrapper `Labeled<T>` (name TBD)
carries both axes
and propagates them through all operations.

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
The scope, confidentiality, and integrity
are encoded in the type,
making all three compile-time constraints.

**Filesystem**: `File` for single files,
`FileTree` for directory trees.
Integrity determined by source:
user-owned directories are high-integrity;
directories containing external input are low.
Confidentiality determined by content:
personal files are private;
shared/public files are public.

**Web**: scoped by domain.
Integrity determined by the domain's
principal tier in the contact book.
Most external domains are World-tier.
Confidentiality is Public
(web content is world-readable by nature).
Domain-level scoping balances specificity
(auditable grants)
with practicality (URLs are dynamic).

**Email**: scoped by account,
optionally filtered by sender.
The sender's principal tier
(from the contact book)
determines the integrity of data read.
An inbox filtered to Friend-tier contacts
produces `Friend`-integrity data.
An unfiltered inbox produces `World`-integrity data.
`EmailOutbox`: sends from a given address;
its `send` method requires data
meeting a minimum integrity level
and compatible confidentiality
(private data cannot be sent
without explicit declassification).
The contact book maps contacts to tiers
and stores per-contact settings and roles,
enabling template-driven composition
(e.g., a "colleague" role
with appropriate tone and signature).

**Sockets**: local paths as IPC endpoints.
Subject to the same permission grants
and labeling as other resources.

These scoping patterns make the permission manifest
specific and auditable:
the user sees exactly which domains,
which email addresses,
and which filesystem paths the plan touches,
along with their integrity tiers.

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

### Information flow model

jev's security model maps directly to
information flow control (IFC),
the same theoretical framework behind
[Jif](https://www.cs.cornell.edu/jif/) (Myers & Liskov)
and [Perl taint mode](https://perldoc.perl.org/perlsec).
Two orthogonal axes, both enforced at compile time:

**Confidentiality**: who can see this data.
Prevents private data from leaking
to world-visible outputs.
Lattice: `Private > Public`
(extensible to domain compartments
like Personal/Work in the future).

**Integrity**: who has endorsed this data.
Prevents low-integrity data
(which may contain adversarial content)
from influencing high-integrity actions.
Lattice: `Me > Friend > World`
(extensible to additional tiers).

These are orthogonal.
A trusted friend's email is high-integrity
(Friend tier) but may be private.
A public Wikipedia page is low-integrity
(World tier) but public.
The axes compose independently.

#### Principals and tiers

Principals are entities with security concerns.
The system is generic over principals
but ships with a fixed initial set:

- **Me**: the user. Highest integrity.
  Data endorsed by Me can do anything.
- **Friend**: a trusted contact tier.
  Configured per-contact in the contact book.
  Can influence medium-sensitivity actions
  (e.g., calendar) but not high-sensitivity ones
  (e.g., financial).
- **World**: unknown/external. Lowest integrity.
  Cannot directly influence any action.

The contact book maps specific contacts to tiers:
`Contact<"alice@example.com">` resolves to
the `Friend` tier at code generation time.
The specific contact identity appears
in the permission manifest for auditability;
the tier is the compile-time type
that all contacts at that level share.

When contacts are not known statically
(e.g., iterating over an inbox),
the resource's configured tier applies.
An inbox filtered to Friend-tier contacts
produces Friend-integrity data.
An unfiltered inbox produces World-integrity data.

Additional tiers (e.g., Inner, Outer)
can be added by extending the principal set.
The lattice ordering determines
which tiers satisfy which requirements.

#### `Labeled<T, C, I>`

All data produced by resources
is wrapped in `Labeled<T, C, I>`,
a monadic type carrying both axes.
Resources carry labels as type parameters
(`File<C, I>`, `FileTree<C, I>`);
data produced by a resource inherits its labels.

Labeled propagates through all data operations:

```rust
// Map preserves labels
let upper: Labeled<String, Private, Me> =
    content.map(|s| s.to_uppercase());

// Combining two values: lattice join.
// Most restrictive confidentiality,
// least trustworthy integrity.
let combined: Labeled<String, Private, World> =
    friend_email    // Private, Friend
        .join(
            web_content,  // Public, World
            |a, b| format!("{}: {}", a, b),
        );
```

This means mixing private data with anything
keeps the result private,
and mixing low-integrity data with anything
taints the result.
Both conservative and correct.

`Labeled::local(value)` creates data
with maximum trust (Public, Me)
for locally-constructed values
not derived from resources.

#### Declassification and accreditation

Two canonical operations for crossing label boundaries.
The inverses (`classify` and `discredit`)
are trivial and may not appear in practice.

**Automatic declassification** via bounded-output types.
When an operation maps labeled input
to a type with bounded, known variants,
the output is clean.
A `Declassifiable` trait marks safe output types:

```rust
trait Declassifiable {}
impl Declassifiable for bool {}
impl Declassifiable for Sentiment {}
impl Declassifiable for u32 {}
// String: NOT Declassifiable
```

An enum with N variants carries at most
log2(N) bits from the input.
Adversarial content can influence
which variant (misclassification)
but the variant itself is a known-good value.
Misclassification is a competence problem,
not a security escalation.

**`declassify`** for the confidentiality axis.
Decreases classification: Private → Public.
Releases private data for world-visible use.
Requires human confirmation for non-Declassifiable types.

**`accredit`** for the integrity axis.
Increases integrity to a target tier.
Endorses data as more trustworthy.
Requires human confirmation for non-Declassifiable types.

```rust
// Declassify private data for public use
let public_data = jevs::declassify!(private_data).await?;

// Accredit World data to Friend integrity
// after human review.
// Can now influence calendar, not bank.
let reviewed = jevs::accredit!(raw_data, jevs::label::Friend).await?;

// Accredit all the way to Me
let endorsed = jevs::accredit!(raw_data, jevs::label::Me).await?;
```

#### Action boundaries

Action-capable resource methods declare
their required integrity and confidentiality
via trait bounds on their inputs.
This is not a separate "action resource" trait;
it's simply that `outbox.send()` requires
data meeting a minimum integrity level
and compatible confidentiality:

```rust
impl EmailOutbox {
    // Send requires Friend+ integrity
    // and Public confidentiality
    // (email goes to the world).
    async fn send<C, I>(
        &self,
        draft: Labeled<Draft, C, I>,
    ) -> Result<()>
    where
        I: SatisfiesIntegrity<Friend>,
        C: SatisfiesClassification<Public>,
    { ... }
}
```

Passing `Labeled<Draft, Private, World>` fails both:
`World` doesn't satisfy `Friend` integrity,
`Private` doesn't satisfy `Public` confidentiality.
Passing `Labeled<Draft, Public, Me>` succeeds.

### Sandboxing

A sandbox is a capability type,
not just "plans run in a container."
It is the universal execution boundary
for all runtime subagents.

**Capability attenuation.**
A parent task constructs a sandbox
by mounting a subset of its own resources.
The sandbox can only contain
resources the parent holds;
you cannot grant what you do not have.
This is the
[object capability attenuation](https://joeduffyblog.com/2015/11/10/objects-as-secure-capabilities/)
pattern.

```rust
fn parent_task(res: &mut Resources) {
    let sb = Sandbox::builder()
        .mount(&res.inbox)          // read
        .mount_mut(&mut res.output) // write
        // outbox deliberately excluded
        .shell(true)
        .build();
}
```

**Sandbox labels are derived from mounts.**
Lattice join across all mounted resources:
most restrictive confidentiality,
least trustworthy integrity.
These labels determine the label
of everything that comes out of the sandbox.

**Taint-capability check at construction.**
If the sandbox's derived integrity
is below what an action resource requires,
mounting that action resource is a compile error.
The type system prevents combining
low-integrity sources with high-integrity actions
in the same sandbox:

```rust
// Fine: World integrity, no action resources.
Sandbox::builder()
    .mount(&untrusted_web)  // World integrity
    .mount_mut(&mut scratch) // passive resource
    .shell(true)
    .build()

// COMPILE ERROR: World integrity + outbox
Sandbox::builder()
    .mount(&untrusted_web)  // World integrity
    .mount(&outbox)         // requires Friend+
    .build()
```

**Shell access** is a grant within the sandbox.
The subagent can run shell commands,
but only has access to mounted paths
and allowed network endpoints.
The container enforces this at runtime,
providing a second enforcement layer
beyond the type system.

**Container enforcement.**
The sandbox maps to a real container:
- Filesystem grants become mounted paths
  (read-only or read-write)
- Network grants become allowed endpoints
- No grant means not mounted, not reachable

`std::fs`, `std::net`, `std::process` etc.
are not useful inside the container
because nothing is accessible
beyond what the grants specify.
Combined with the RuntimeKey barrier
(task code cannot construct resources),
this closes the escape hatch.
The type system is the primary defense;
the container is belt to the type system's
suspenders.

**Sandbox nesting.**
A subagent within a sandbox can create
sub-sandboxes and delegate further.
The same attenuation rule applies:
each level can only grant
what it currently holds.
Capabilities monotonically decrease
down the delegation chain.

**Output.**
Subagents write output to mounted storage
(local filesystem, pipes, or MCP calls),
not return values.
This naturally supports multiple outputs
and streaming.
All output carries the sandbox's labels.

### Permission manifest

The manifest lists declared resources
and any trust boundary crossings:

```
Resources:
  File    read   /data/**             priv me     (summarize)
  File    write  /output/report       priv me     (root)
  Inbox   read   luis@x.com [alice]   priv friend (scan)
  Outbox  send   luis@x.com           pub  friend (notify)

Label crossings:
  accredit   World -> Friend  inbox summary (scan)
  declassify Private -> Public  notification body (notify)
```

No label crossings = auto-approved.
Label crossings present = user confirms each.

### Resource declarations

The LLM outputs a single ```rust``` block.
Resources are declared inline
via the `#[jevs::needs(...)]` attribute macro:

```rust
use jevs::{File, FileTree, Labeled};
use jevs::label::*;

#[jevs::needs(
    fs: FileTree<Private, Me> = "./",
    config: File<Private, Me> = "./config.toml",
)]
pub async fn root(
    needs: &mut Needs,
) -> anyhow::Result<()> {
    let cfg = needs.config.read().await?;
    // ...
    Ok(())
}
```

The macro generates a `Needs` struct,
a `create(&RuntimeKey)` function,
and `linkme` distributed slice entries
for the permission manifest.
The orchestrator does not parse declarations;
the compiled binary is self-describing.

**Path conventions.**
Trailing `/` distinguishes directories from files:
`"./data/"` = `FileTree`, `"./data.txt"` = `File`.
Future resource kinds will use URL schemes:
`https:` for web, `mailto:` for email, etc.

**Labels in declarations.**
Classification and integrity
are type parameters on the resource type.
The planner chooses labels
to match the data's origin and trust level:

```rust
#[jevs::needs(
    data: File<Private, Me> = "/data",
    web: File<Public, World> = "/cache/page.html",
)]
```

Defaults when not specified:
`Private` classification, `Me` integrity.

**Stash** is not a resource;
it's a task-local helper, no grant needed.

**Re-exports.**
`jevs` re-exports common types at the crate root:
`jevs::File`, `jevs::FileTree`,
`jevs::Labeled`, `jevs::RuntimeKey`.
Labels stay under `jevs::label::*`.

### Plan project structure

```
plans/<id>/
├── Cargo.toml
└── src/
    ├── main.rs    -- fixed shim (embedded asset)
    └── tasks.rs   -- LLM-generated (#[jevs::needs])
```

`main.rs` is a fixed embedded asset.
It shows resource needs and crossing count,
prompts for approval
(default Y if no crossings, N if crossings;
't' to view the tasks source with 4-space indent),
then calls `jevs::gate::init()?`
for per-crossing a/p/r decisions,
then initializes a random RuntimeKey,
calls `tasks::create(&key)`,
and passes the result to `tasks::root()`.
Same for every plan.

`tasks.rs` is the single LLM output.
The `#[jevs::needs(...)]` macro
generates the `Needs` struct and `create()` function.
It also registers needs in a distributed slice
so `manifest::init()` can display them.

## Implementation phases

Phases are ordered to maximize feedback
from real personal-assistant usage.
The safety foundation (Phase 2) lands first
so every subsequent resource is safe from day one.
The information flow model (Phase 3)
and real-world resources land together
because resource types need labels
and labels need resources to be useful.
Sandboxing (Phase 4) builds on the label model;
task trees and jev planner subagents (Phase 5)
build on sandboxing.

**Phase 1 (done): Core scaffold**
- Filesystem, text, and label types in jevs
- CLI with plan/run/go commands
- Single-shot planning with compile-error retry
- Single `main.rs` per plan (flat, no task tree)
- LLM exchange logging for prompt iteration

**Phase 2 (done): Safety foundation**
- RuntimeKey barrier: `init(random)` once at startup,
  `File::open(&key, root)` requires the key,
  tasks never receive it
- Single LLM output: one ```rust``` block
  with `#[jevs::needs(...)]` attribute macro
- `jevs-macros` proc macro crate
  generates `Needs` struct, `create()`,
  and distributed slice registrations
- `jevs::manifest` module: `Need` struct,
  `NEEDS` distributed slice, `init()` prompt
  (same pattern as `gate::init()`)
- Fixed `main.rs` shim (embedded asset)
  calls `manifest::init()` then `gate::init()`
  then `RuntimeKey::init()`
- Re-exports: `jevs::File`, `jevs::FileTree`,
  `jevs::Labeled`, `jevs::RuntimeKey`, `jevs::needs`
- Path conventions for resource identification
  (`"./"` = directory, `"./config.toml"` = file)
- Stash: plan-local blob storage,
  created by tasks directly (no key required)
- Per-module `API_DOCS` + `jevs::api::catalog()`
- Cargo.toml as embedded template asset

**Phase 3: Information flow model + resources**
- Two-axis information flow model:
  confidentiality (Private/Public)
  and integrity (Me/Friend/World) **(done)**
- `Labeled<T, C, I>` monadic wrapper
  with lattice join semantics **(done)**
- `Declassifiable` trait
  for automatic declassification
  of bounded-output types **(done)**
- `jevs::declassify!` / `jevs::accredit!` macros
  with `linkme` distributed slice registration
  and per-crossing gate decisions **(done)**
- `SatisfiesClassification` / `SatisfiesIntegrity`
  trait bounds on resource methods **(done)**
- Confidentiality/integrity fields
  in resource declarations **(done)**
- `File<C, I>` / `FileTree<C, I>`
  with labeled read/write **(done)**
- Human confirmation gate via `jevs::gate` module:
  `init()` collects decisions at startup,
  `check()` enforces at runtime,
  auto-approve when no crossings **(done)**
- Principal tiers: generic machinery,
  fixed initial set (Me, Friend, World)
- Contact book: maps contacts to tiers,
  stores per-contact settings and roles
- Real-world resource types:
  web, email (inbox/outbox), calendar
- jevu: user utility library
  (curate reusable functions from prior plans)
- Saved plans + rerunning

**Phase 4: Sandbox + subagent model**
- Sandbox as capability type
  (builder pattern, mount resources,
  derived labels, taint-capability check)
- Vanilla subagents
  (LLM in sandbox, shell access as grant,
  model selection)
- Container enforcement
  (sandbox maps to real container config)
- Sandbox nesting
  (recursive delegation,
  monotonic capability attenuation)
- Output via mounted storage

**Phase 5: Task trees + jev planner subagents**
- Expand-down / resolve-up planning loop
- Per-node resource structs with propagation
- Parallel leaf implementation
- Mechanical resource hoisting for shared access
- Jev planner subagents
  (nested planning loop, compilation as
  integrity endorsement, approval only
  for new grants or label crossings)
