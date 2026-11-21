# warp-flow publishes a door of its own

_Status: accepted._

warp-flow runs the same token-authenticated NDJSON door as its siblings, at
`~/.warp/flow/` `ipc.port` and `ipc.token`, and it is the only writer of a run's
state. warp-agent's Flow page reads that door, and both UIs send their answers
to it — `runs`, `run`, `answer`, `pause`, `resume`, `abandon` and `subscribe`.
The deciding factor is that two surfaces must not become two sources of truth:
a second UI that read `.scratch/` directly would drift from the engine's live
state and could not answer a question that has not been written to disk yet, and
a second UI that wrote the markdown would race the engine's own writes.

Considered options: warp-agent reads the issue folder only (no live events, no
way to answer); file polling from warp-agent (no events, and two writers);
folding warp-flow's UI into warp-agent (one app, but the engine's own surfaces —
the board, the graph editor — would live in a chat app).

Consequences: the two apps now hold a door each, and either must keep working
when the other is absent; the flow door's version moves with warp-agent's Flow
page in one window; a question answered on either surface lands in one place,
and a run keeps pausing rather than failing while the door is down.
