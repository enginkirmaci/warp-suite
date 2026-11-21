# Workflows are declarative and revisable

_Status: accepted._

A workflow is a TOML graph of steps, transitions and gates. A run starts from a
shared default shipped with warp-flow and instantiates a copy at
`.scratch/<feature-slug>/workflow.toml`; the engine reads the copy. Every change
made while the run is live — by a person or by the engine — appends a revision
carrying its number, author and reason, and an earlier revision is never
rewritten. The engine reads the copy at each step boundary, so an edit made while
a step runs lands on the next one and never inside a running session. The
deciding factor is that "change the flow while it runs" then
becomes one operation with one record, so a human editing the file and an engine
inserting a step are the same mechanism, and the revision list answers why a run
took an unusual path.

Considered options: a fixed pipeline in code (simple, but a new path needs a
rebuild and a live change is impossible); a Rust trait per step as a code DSL
(type-safe, but a user cannot author a step without Rust and a run cannot
rewrite itself); YAML (equivalent in power, but the suite's config files are
TOML).

Consequences: the engine owns a graph interpreter and a revision writer; the
TOML schema becomes a public interface and needs its own version field; an
invalid revision can stall a run, so the interpreter validates before adopting
one.
