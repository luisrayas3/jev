# Vision

jev is an AI assistant you can trust
with your real resources —
email, calendar, files, messages, accounts —
because permissions are compiler-enforced,
not prompt-based.

## The trust problem

AI assistants face a binary choice:
they either can't access your real stuff
(safe but useless for real work),
or they get broad access with no guardrails
(useful but dangerous).

No current architecture lets you say
"read my email and calendar,
but do not send anything or modify anything"
and have that *enforced*.
Today, that boundary is a prompt instruction
the model might follow.
jev makes it a constraint
the compiler proves before anything runs.

## How it works (in brief)

When you describe a task,
jev decomposes it into a tree of subtasks,
each declaring what resources it needs.
All resource requirements bubble up
to a single permission manifest —
a flat, readable list of grants
(read `/data`, write `/output`, etc.).
You approve the permissions,
and the compiler guarantees
the plan can't exceed them.

Resource constructors live in a dedicated module
that only the orchestrator controls.
Task code receives resources as function parameters
and literally cannot construct new ones —
the symbols aren't available at compile time.
The plan runs in a container
where only approved resources are mounted.

Deterministic work — filtering, transforming,
aggregating — runs as compiled native code.
Reasoning work runs as LLM calls,
each scoped to only the resources it needs.
The right-sized model handles each subtask:
a small model for classification,
a frontier model for complex reasoning.

See [architecture.md](architecture.md)
for technical design details.

## Use cases

### Personal automation with real resources

An AI assistant you trust enough
to give access to your actual email, calendar,
files, messages, and accounts.

*Go through my last 50 emails,
find anything that looks like an action item,
cross-reference with my calendar for conflicts,
and draft replies for the ones
that need scheduling.*

This touches email (read-only),
calendar (read-only),
and drafts (write-only).
The plan proves those access boundaries
before anything executes.

### Multi-resource coordination

Tasks that reach across multiple resources
where the blast radius of a mistake is largest.
Email + calendar + files + messaging,
each scoped to exactly the access required,
even when multiple resources
and multiple reasoning steps are in play.

*Check my calendar for meetings this week,
pull the relevant docs from shared drives,
summarize prep notes for each meeting,
and add them to my task list.*

### Repeatable batch automation

Tasks that run over volume or on a schedule.
Compilation cost is invisible
against the planning step
and amortized across runs.
Deterministic work runs as native code,
not wasted LLM calls.

*Every Monday, scan receipts from email,
extract amounts and vendors,
cross-reference against the budget spreadsheet,
and file a summary.*

### High-stakes operations

Any domain where the cost of an agent
doing the wrong thing is high:
financial automation, compliance workflows,
infrastructure management.
Compiler-verified access patterns
and readable, auditable plans
provide confidence
that runtime-checked frameworks cannot match.

## The experience

Describe a task in natural language.
The system decomposes it, plans each piece,
and presents a permission manifest:
what resources the plan will access,
with what access mode, and why.
You approve the permissions and it executes.

You don't need to read generated code.
The compiler proved the code can't exceed
the permissions you approved.
The container enforces it at runtime.

Plans are saveable, rerunnable,
versionable, and shareable.
The plan is the artifact —
auditable, reproducible, and proven safe.
