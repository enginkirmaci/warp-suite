# The door speaks a harness namespace

_Status: accepted._

The warp-agent door exposes the app-level harness operations through one
`{"cmd":"harness","method":"…","params":…}` command rather than one top-level
`cmd` per operation, and a `subscribe` command pushes harness events on the same
connection. The existing `ping`, `open-workspace`, `new-chat`, `run-prompt`,
`cancel`, `models` and `status` commands stay for compatibility. The deciding
factor is that the door is a cross-app contract with Warp Station: one
namespace keeps the top-level vocabulary small, lets the method table mirror the
`Harness` trait, and versions in one place; ~30 flat commands would scatter the
same surface across the protocol and the Station client.

Considered options: flat commands (simplest dispatch, but a wide top-level
vocabulary and a commensurately wide Station client) and JSON-RPC (standard, but
a breaking rewrite of a working NDJSON door for no sibling benefit).

Consequences: the door protocol version moves and Warp Station's client lands in
the same change window; `run-prompt` remains the one-shot convenience while
`harness` + `subscribe` is the live/streaming path; every new harness operation
adds a method, not a top-level command.
