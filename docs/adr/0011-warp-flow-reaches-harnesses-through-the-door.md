# warp-flow reaches harnesses through the warp-agent door

_Status: accepted._

warp-flow does not spawn `opencode` or `dsh` and does not speak either
harness's protocol itself. It attaches to warp-agent's door — `~/.warp/agent/`
`ipc.port` and `ipc.token`, protocol v3 — and drives sessions with
`harness create`, `harness send_prompt`, `subscribe`, `cancel` and `status`. The
deciding factor is that the door is already a versioned cross-app contract that
carries session management, streaming, permission cards and cancellation; using
it needs no adapter work, while the alternative moves three compiled-in adapters
and their process lifecycle out of warp-agent.

Considered options: a direct harness client inside warp-flow (works without a
GUI, but duplicates or relocates all three adapters and takes over their
lifecycle); lifting the adapters into a shared kit crate now (cleanest in the
long run, but a large change to a working app before the engine exists).

Consequences: warp-agent must be running for any step to execute, and a run
pauses rather than fails while the door is unavailable; the door contract moves
in one window with the warp-flow release; a session made with `harness create`
never appears as a chat or a workspace row, so flow sessions stay out of the
owner's sidebar — as long as `open-workspace` is never called (measured).
