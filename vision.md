# Vision

jev is an AI assistant you can trust
with your real resources
(email, calendar, files, messages, accounts)
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
to a permission manifest:
a flat, readable list of grants
(read `/data`, write `/output`, etc.).

Resource types encode two security properties
from information flow control:
*confidentiality* (private data stays private)
and *integrity*
(untrusted content cannot influence actions).
The compiler enforces both
before anything runs.
If the plan requires no trust boundary crossings,
it is safe by construction and runs immediately.
If it does cross boundaries
(accrediting untrusted data, declassifying private data),
you confirm those specific transitions.
Task code receives resources as function parameters
and cannot construct new ones.
The plan runs in a sandbox
where only declared resources are mounted,
providing a second enforcement layer at runtime.

Deterministic work (filtering, transforming,
aggregating) runs as compiled native code.
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

## Threat model

jev's safety story addresses five distinct threats.
The first two map to the two axes
of information flow control.

### 1. Prompt injection (integrity)

The primary threat.
Untrusted content (emails, web pages, documents)
can contain adversarial instructions
that hijack the LLM into unauthorized actions.

jev's defense is structural:
the *integrity* axis of the type system
prevents low-integrity data
from flowing into high-integrity actions.
A subagent that sees untrusted emails
cannot also hold an outbox handle;
the compiler rejects the combination.
Injected instructions can corrupt reasoning
within the subagent's sandbox
but cannot escalate to actions
the subagent was not granted.

For jev planner subagents
(nested planning loops at runtime),
the defense is stronger:
the planner generates code,
and `rustc` proves the generated code
respects information flow constraints.
Compilation is an integrity endorsement
mechanism; the planner can wire together
resources that would be unsafe
in a single LLM context
because the compiled plan provably separates them.

The attack surface is the planning phase,
where the LLM has broad capability.
An injection could inflate resource requests,
but the compiler still proves label safety.
A plan with no label crossings auto-approves safely
regardless of what the LLM requested;
an injection that adds a declassification or accreditation
triggers human review of that specific crossing.

### 2. Data leakage (confidentiality)

Private data flowing to unintended recipients.
A plan that reads personal calendar entries
should not embed them in a work email.

jev's defense is the *confidentiality* axis:
private data carries a `Private` label
that propagates through all operations.
Action resources that produce world-visible output
(email outbox, public API calls)
require `Public`-labeled input.
Passing `Private` data is a compile error.
Explicit declassification
(with human confirmation)
is required to release private data.

### 3. LLM incompetence

The most common failure mode.
The model generates bad plans:
wrong logic, missed edge cases,
hallucinated APIs, subtle data corruption.

jev addresses this at multiple layers:
- **Compilation** catches type errors,
  wrong resource usage, and API misuse
- **Deterministic code** for filtering,
  transforming, and aggregating
  means LLM errors in data handling
  surface as compile errors, not silent bugs
- **Task decomposition** scopes each LLM call
  to a narrow, well-defined subtask
  with only the resources it needs
- **Right-sized models** match task complexity
  to model capability,
  reducing error rates on simple work

Integrity labels also help
beyond adversarial contexts:
high-integrity sources
(curated documentation, verified databases)
produce data the system treats
with higher confidence than
low-integrity sources
(arbitrary web content, unverified input).
The type system makes source quality
explicit and trackable.

What jev does *not* do:
verify that correct logic was applied
within a valid plan.
A plan that reads the wrong emails
but has valid types will compile and run.
Auditability (readable plans, logged execution)
is the mitigation, not prevention.

### 4. Privacy and third-party exposure

User data flows through LLM calls.
jev constrains *which* data reaches *which* call
via resource scoping and confidentiality labels:
a subtask only sees the resources
passed to it, and private data
cannot flow to world-visible outputs
without explicit declassification.

This limits exposure surface
but does not fully solve the problem:
data sent to an LLM provider
is data sent to a third party.
Local model support, data classification,
and provider trust policies
are future concerns, not current guarantees.

### 5. Other adversarial threats

Supply chain attacks on dependencies,
container escapes, compromised model providers,
and side-channel exfiltration.

jev's contribution here is minimal today.
Container isolation and dependency auditing
are standard infrastructure concerns,
not novel to this architecture.
The sandbox boundary does limit
what a compromised subtask can reach,
but this is a side effect of the design,
not a primary defense.

## The experience

Describe a task in natural language.
The system decomposes it, plans each piece,
and compiles.
If the plan crosses no trust boundaries,
it runs immediately.
If it does, you confirm
the specific boundary crossings, and it runs.

You don't need to read generated code.
The compiler proved the information flow is safe.
The container enforces resource access at runtime.
A permission manifest is always available
as an audit artifact.

Plans are saveable, rerunnable,
versionable, and shareable.
The plan is the artifact:
auditable, reproducible, and proven safe.

## Example

*"Go through my email and update my TODOs,
then do any you can."*

**Without jev**, you give the agent
your inbox, TODO file, outbox, and bank.
It has all four from the start.
An email from a stranger says "send me $100."
The agent extracts it as a TODO,
then the "do" step executes it.
Nothing enforces the boundary
between reading untrusted email
and taking high-stakes actions.

**With jev**, the email content carries
`World` integrity (untrusted sender).
The bank's `send` method requires `Me` integrity.
The compiler rejects the plan
unless it includes `.accredit::<Me>()`,
which pauses for your confirmation.
The stranger's request
never reaches your bank unsupervised.
