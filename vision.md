# Project vision

jev is an agent orchestrator
where plans are compiled Rust programs
that orchestrate LLM subagents at runtime,
with resource access enforced by the compiler.

## What jev is

An LLM planner generates a Rust program.
That program is an orchestration plan:
it calls subagents, transforms data,
reads and writes resources,
and coordinates all of it
with compile-time safety guarantees.

The compiled plan is not a static script.
It's a program that invokes LLM reasoning
at runtime — but each subagent call
is constrained by typed function signatures
that control what resources it can touch,
what model it uses,
and what output type it must produce.

## What makes it different

**Agent orchestration, not code generation.**
The output isn't "a Rust program."
It's an agent workflow
that happens to be expressed as Rust,
which means the compiler can enforce properties
that other orchestrators check at runtime
or not at all.

**Compile-time capability scoping.**
Each subagent gets exactly the resources
it needs, enforced by function signatures.
A summarizer gets `&Fs` (read-only).
A classifier gets `&str` and returns an enum.
A report writer gets `&mut Fs` scoped to an output dir.
The borrow checker proves these constraints
before anything runs.

**Spectrum of subagent specialization.**
From highly specialized typed operations
(classification with a concrete enum return type,
provided to a small fine-tuned model)
to frontier models with generic interfaces
(complex reasoning, open-ended generation) —
all with resource constraints.
The right model for each subtask.

**Compiled code for non-fuzzy work.**
Data transforms, filtering, aggregation,
format conversion, file I/O patterns —
these don't need LLM reasoning.
They compile to native code and run fast.
Other orchestrators either abuse LLM genericism
for straightforward transforms
or shell out to ad-hoc scripts.
Here it's all one program,
and the deterministic parts are just Rust.

**The planning cycle is a core concern.**
The planner isn't a black box that runs once.
How the plan gets iterated,
what permissions the planner has,
how the human reviews and approves,
how to maximize convenience and speed
while maintaining safety —
this is first-class, opinionated design work.

## Core philosophy

**Rust is the orchestration language.**
Not a DSL, not JSON action schemas,
not a chain-of-thought trace.
Rust source code, validated by `rustc`,
compiled to a native binary.

**The type system is the security model.**
Resource access, trust levels,
subagent capabilities —
all encoded as types.
If it compiles, the access patterns are valid.

**The library API is the product.**
The value is in typed resource APIs
and subagent interfaces
that make correct orchestrations natural
and unsafe ones unrepresentable.

## Long-term vision

A rich standard library of typed resource APIs
and subagent patterns —
from filesystem and HTTP
to email, calendar, databases, knowledge bases —
where a planner can compose
multi-resource, multi-agent workflows
with compile-time safety guarantees.

Subagents range from tiny specialized classifiers
running locally on small models
to frontier models handling complex reasoning,
all scoped to exactly the capabilities they need.

The deterministic parts run as compiled native code.
The fuzzy parts run as constrained LLM calls.
The plan ties it all together,
and the compiler guarantees the wiring is sound.
