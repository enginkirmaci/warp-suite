# The engine never calls a model

_Status: accepted._

warp-flow-core holds no model client and no code that talks to a model
provider. Every step that needs judgement is a harness session: the engine
composes the step's skill document and inputs into a prompt, has warp-agent run
it, and then decides from what the session wrote and from what the gates
returned. The deciding factor is that the engine has to be able to answer why a
step ran, why it repeated and why it stopped, and that answer is only sound when
the decision layer is deterministic and the non-determinism sits behind one
boundary — the harness.

Considered options: an LLM client inside the engine (fastest first version, but
a second model surface with its own credentials, permissions and streaming, and
a core that is non-deterministic by construction); calling the harness HTTP APIs
directly from warp-flow (no GUI dependency, but it moves three compiled-in
adapters out of warp-agent, against ADR-0002).

Consequences: every step costs a harness turn, so step granularity is a budget
decision; step quality becomes a skill/prompt problem rather than a model-API
one; the engine's own tests run with a fake harness and no network.
