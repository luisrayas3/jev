# Project vision

jev is an agent orchestration system
that uses Rust as its plan representation.
An LLM planner generates Rust code
against a typed library of resource APIs,
and `rustc` enforces safety
before anything executes.

## Core project focus & uniqueness

Most agent frameworks use custom DSLs,
JSON-based action schemas,
or interpreted plan representations.
jev uses Rust source code directly.

This means:
- The borrow checker enforces resource access safety
  (no concurrent read+write)
- Trust levels are types
  (`Unverified<T>` vs `Verified<T>`)
- The compiler catches invalid plans
  before execution
- No custom IR, no interpreter,
  no runtime permission checks —
  safety is structural

The library is the product.
Well-designed type signatures
make safe programs easy to write
and unsafe programs fail to compile.

## Core philosophy

**Rust is the whole story.**
No custom compiler, no interpreter.
The planner produces Rust code,
`rustc` validates it,
and the compiled binary runs.

**The type system is the security model.**
Resource access, trust levels,
and operation permissions
are encoded as types.
If it compiles, the access pattern is valid.

**The library API is the product.**
The value is in signatures
that make correct programs natural
and incorrect programs unrepresentable.

## Long-term vision

A rich library of typed resource APIs
(filesystem, email, calendar, HTTP,
databases, knowledge bases)
where an LLM planner can compose
complex multi-resource workflows
with compile-time safety guarantees.

The planner gets full LLM reasoning power
but is sandboxed:
it can only produce code
that `rustc` must accept.
