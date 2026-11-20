# Harness adapters: one interface for opencode v1, opencode v2 and dsh

Status: ready-for-agent

## Problem Statement

warp-agent runs chats on two coding-agent runtimes, opencode v1 and dsh, but the
harness is a tag compared across the store, the UI and the door rather than a
boundary. The dsh client's request and event variants live in the opencode
client's shared enums; settings hold two fixed executable fields; the composer
pill is a two-way toggle; the door reports a fixed two-element list; and every
feature re-derives which server to talk to. The same tag is compared in roughly
a hundred and thirty non-test places across thirteen modules.

From the user's side: I cannot turn a harness off. I cannot add opencode v2 — a
different `/api` surface, a different SSE event vocabulary, a shared background
service and mandatory Basic auth — without editing those same modules. And every
new chat feature has to be checked and wired for each harness separately. Warp
Station, the sibling app that drives warp-agent's chats through the door, carries
the same fixed two-harness assumption in its AI card and its client.

## Solution

One harness interface, one adapter per harness, one registry. A `Harness` trait
(wide, app-level: about thirty methods) is implemented by `opencode` v1,
`opencode-v2` and `dsh`; a compile-time registry is the only list of what the
app can run, and a `HarnessHost` owns the network runtime and the per-harness
instances. The store and the UI consume one typed `HarnessEvent` stream and
never see a harness protocol.

Settings become a harness map with an enable switch and an executable, so a user
can run only the harnesses they want. Disabling a managed harness stops its
processes; disabling the attached v2 connection disconnects it and leaves the
shared service running. Open chats on a disabled harness stay readable and
become read-only.

Three adapters ship. `opencode` v1 keeps today's behaviour (one managed
`opencode serve` per workspace). `opencode-v2` attaches to the user's shared
OpenCode v2 service, discovered through the CLI and its registration file and
started on demand. `dsh` keeps today's managed singleton. All app features —
models, sessions, prompts and streaming, permissions, questions/forms, commands
and skills, diffs, subagents, inbox, revert, shell, MCP — run through the
interface, with per-adapter capabilities deciding what appears.

The door exposes the same operations through one `harness` command with a method
name and a `subscribe` event stream; the existing commands stay. Warp Station is
updated in the same change window as a run-loop client: harness picker, models,
new chat/run prompt, and the permission/form prompts that block a run it
started. The door token remains the only gate, and auto-allow is unchanged.

## User Stories

1. As a warp-agent user, I want to see every harness the app knows about in Settings, so that I understand what my chats can run on.
2. As a warp-agent user, I want to enable or disable a harness from Settings, so that I only see and run the ones I use.
3. As a warp-agent user, I want the settings page to be generated from the harness registry, so that a new harness appears without a hand-written row.
4. As a warp-agent user, I want to choose a default harness for new chats, so that I do not pick it every time.
5. As a warp-agent user, I want the default harness to fall back to another enabled harness when the chosen one is disabled, so that new chats still work.
6. As a warp-agent user, I want the last enabled harness to be un-disablable, so that the app never ends up with no backend.
7. As a warp-agent user, I want a disabled managed harness to stop its processes, so that it uses no CPU or memory.
8. As a warp-agent user, I want a disabled attached harness to disconnect without stopping the shared service, so that my other OpenCode clients keep working.
9. As a warp-agent user, I want open chats on a disabled harness to stay readable, so that I do not lose their history.
10. As a warp-agent user, I want those chats to refuse new sends with a clear note, so that I know why and how to re-enable the harness.
11. As a warp-agent user, I want to be asked before a disable cancels a running turn, so that I do not lose work by accident.
12. As a warp-agent user, I want the composer pill to open a menu of every enabled harness, so that I can pick any of them instead of toggling between two.
13. As a warp-agent user, I want the harness pill to lock once a chat has a session, so that the backend cannot change mid-chat.
14. As a warp-agent user, I want the sidebar row to show which harness a chat runs on, so that I can tell them apart.
15. As a warp-agent user, I want models, agents, commands and skills to be scoped to the chat's harness, so that a model from another harness can never be sent.
16. As a warp-agent user, I want streaming answers on every harness, so that a chat feels live regardless of backend.
17. As a warp-agent user, I want permission prompts on every harness, so that I control tool use.
18. As a warp-agent user, I want my auto-allow settings to keep working across harnesses, so that my existing approval preferences still apply.
19. As a warp-agent user, I want v2 permissions to keep their once/always/reject decision, so that v2's three-way choice is not flattened to a boolean.
20. As a warp-agent user, I want v2 forms to appear as question cards, so that structured questions work in the chat.
21. As a warp-agent user, I want multi-field forms to be answerable, so that a complex form does not block a turn.
22. As a warp-agent user, I want slash commands to work on every harness, so that the composer is consistent.
23. As a warp-agent user, I want to list and activate skills on v2, so that skill activation works.
24. As a warp-agent user, I want tool diffs on every harness, so that I can see what the agent changed.
25. As a warp-agent user, I want to fork, rename and delete sessions on every harness, so that chat management is consistent.
26. As a warp-agent user, I want to compact a long chat on every harness, so that context stays manageable.
27. As a warp-agent user, I want v2 subagent child sessions to appear in the transcript and sidebar lineage, so that background work is visible.
28. As a warp-agent user, I want the v2 inbox surface, so that queued and steered input is visible and controllable.
29. As a warp-agent user, I want v2 revert controls, so that I can stage, commit or clear a revert.
30. As a warp-agent user, I want a v2 shell surface, so that I can run a shell command in the session's directory.
31. As a warp-agent user, I want v2 background sessions, so that a long-running session can keep working in the background.
32. As a warp-agent user, I want MCP servers listed on every harness, so that I know what tools are connected.
33. As a warp-agent user, I want to add, connect, disconnect and remove MCP servers on v2, so that v2 MCP management works.
34. As a warp-agent user, I want the v2 service to be discovered automatically, so that I do not have to copy a URL and password into settings.
35. As a warp-agent user, I want the v2 service to be started when it is stopped, so that the first chat works without a terminal command.
36. As a warp-agent user, I want a clear unavailable state when the v2 CLI, service or credentials are missing, so that I can fix it.
37. As a warp-agent user, I want v2 chats to run in the correct workspace directory or worktree, so that repository work is not mixed up.
38. As a warp-agent user, I want my existing settings, chats and model cache to survive the upgrade, so that nothing is lost.
39. As a warp-agent user, I want a disabled harness's cached models to stay out of the pickers, so that I do not choose an unusable model.
40. As a Warp Station user, I want the AI card's harness picker to include opencode v2 when it is enabled, so that I can run wands on it.
41. As a Warp Station user, I want v2 models and variants in the picker, so that model choice works there too.
42. As a Warp Station user, I want a run I start to surface the permission or form prompt that blocks it, so that the run does not hang silently.
43. As a Warp Station user, I want a disabled or unavailable harness to show as such, so that I know why it is missing.
44. As a Warp Station user, I want my existing v1 and dsh runs to behave as before, so that the upgrade does not regress them.
45. As a door client, I want one `harness` command with a method and params, so that I can call any harness operation without a new top-level command per feature.
46. As a door client, I want a `subscribe` stream, so that I can follow runs, permissions, forms and status without polling.
47. As a door client, I want disabled or unavailable harness operations rejected with a typed error, so that I can explain the state to the user.
48. As a door client, I want the existing door commands to keep working, so that older clients do not break.
49. As a door client, I want the protocol version in `ping`, so that I can negotiate the new surface.
50. As a developer, I want to add a harness by implementing one trait and adding one registry entry, so that I do not edit the store, UI or door.
51. As a developer, I want a capability descriptor per adapter, so that unsupported features are hidden or rejected consistently instead of failing at runtime.
52. As a developer, I want an adapter conformance suite, so that a new adapter is proven against the same contract as the existing ones.
53. As a developer, I want the v2 adapter tested against a fake HTTP/SSE server, so that tests do not need a live service.
54. As a developer, I want the door protocol and its version documented, so that both apps can move together safely.

## Implementation Decisions

### The harness interface

- `Harness` is a wide, app-level trait: one method per app-level operation, not
  one per v2 HTTP operation. Categories: lifecycle (start, stop, health);
  catalogs (models, providers, agents, commands, skills); sessions (list, get,
  create, fork, rename, delete); turns (send prompt, interrupt, compact, run
  command, activate skill); approvals (list/reply permissions, list/reply forms
  and questions); diffs (session diff); v2-only operations (inbox
  list/update/cancel, revert stage/commit/clear, shell, background session); MCP
  (list, add, connect, disconnect, remove); and one event stream per instance.
- Every adapter implements every method; an unsupported operation returns a
  typed `Unsupported` error. Capabilities are data, not a second trait set.
- `HarnessCapabilities` declares what an adapter supports: commands, skills,
  skill activation, forms/questions, permissions, diffs, subagents, inbox,
  revert, shell, background sessions, MCP read, MCP management, compaction,
  fork, and whether the connection is managed or attached and multi-location.
  The UI and door render or enable from this descriptor; it is not a user
  switch.
- `HarnessId` is a stable wire string (`opencode`, `opencode-v2`, `dsh`); the
  registry, not an enum, is the list of what exists. The old harness enum is
  retired, and its two serialized values remain valid.
- `HarnessDescriptor` carries the id, display label, default executable,
  connection kind, capabilities and a factory. The registry is a compiled-in
  list; adding a harness is one adapter module plus one descriptor entry, after
  which the settings page, composer pill, door list and model pickers pick it
  up.
- `HarnessHost` owns the network thread/runtime, the instance table, routing,
  enabled state, lifecycle (managed spawn/stop, attached discovery/start), idle
  sweep, restart and shutdown. Adapters own their protocol, transport and I/O
  tasks; they do not own the host runtime.
- Instance scope is a descriptor fact: opencode v1 is one server per workspace;
  dsh and opencode v2 are one app-wide instance; v2 routes a session to a
  directory through its location parameter.

### Events

- One typed `HarnessEvent` stream per harness instance. Variants cover
  server/health state, session status and idle, session created/renamed/deleted,
  message and part snapshots, tool-call state, permission asked/replied,
  question asked/replied or form created/replied/cancelled, catalog loaded,
  subagent/child-session updates, inbox, revert, shell, MCP status and
  structured errors.
- Adapters translate their protocol into this vocabulary; the store and UI never
  parse a harness wire format.
- The v2 adapter accumulates `(assistantMessageID, ordinal)` text and reasoning
  deltas into full-text part snapshots, the shape dsh already synthesizes; the
  transcript's part-based rendering and virtualization do not change.
- Control results (catalog lists, session lists, create/fork replies) remain
  typed request answers; the event stream carries what changes after a request.
- A subscription starts before a prompt is sent, so the first deltas cannot race
  the prompt acknowledgement.

### Adapters

- `opencode` v1 is a behaviour-preserving extraction of today's client: one
  managed `opencode serve` per workspace, attach to a healthy server, 25 s health
  wait, ask-forcing config, SSE event pump with the reconnect ladder, REST
  catalogs/sessions/prompts/permissions/questions/commands/diffs, and idle stop.
- `opencode-v2` attaches to the user's shared background service. Discovery runs
  the configured `opencode` CLI's `service status`; a URL means running and
  `stopped` means not. The registration file in the platform state directory
  (`XDG_STATE_HOME` or the platform equivalent, channel-suffixed off the default
  channel) supplies `{url, pid, password}`; a 401 re-reads it. When enabled and
  stopped, the adapter runs `service start`, retries a bounded number of times,
  then attaches. Authentication is Basic with username `opencode` and the
  registration password; the password is never written to settings. The adapter
  confirms `/api/info`, opens the `/api/event` SSE stream, and sends
  `location[directory]` on calls. It maps v2 sessions, prompt/inbox, interrupt,
  compact, commands, skills, permissions, forms, diffs, subagent child sessions,
  inbox, revert, shell, background sessions, MCP list and MCP management.
  Pairing and remote clients are out of scope.
- `dsh` is a behaviour-preserving extraction of today's client: managed
  `dsh web` singleton, launch-token discovery, cookie exchange, WebSocket
  multiplexed streams, event folding into the same typed vocabulary, and
  sessions/commands/questions/models/agents/diffs.
- Disable semantics: a managed harness is stopped, with a confirmation if a turn
  is running; an attached harness interrupts its own running turns after
  confirmation, disconnects and hides, while the shared service keeps running.
  Open chats become read-only with a note. The last enabled harness cannot be
  disabled. The default harness falls back to the first enabled one.
- Re-enabling refreshes catalogs and makes the chats sendable again; cached
  models stay out of pickers while disabled.

### Settings and migration

- Settings gain a harness map keyed by harness id, each entry
  `{enabled, executable}`, plus a default harness. Defaults: `opencode` and
  `dsh` enabled, `opencode-v2` disabled; executables default per descriptor.
- The existing executable fields and default-harness value migrate into the map
  unchanged; unknown keys are preserved; the schema version moves. The UI-state
  session-to-harness map keeps its string values and gains `opencode-v2`.
- The model cache is keyed by `(harness, model)` and remains disposable;
  disabled harness entries stay on disk but out of pickers.
- No v2 URL or password is stored in settings; the registration file is read at
  start and after a 401.
- The settings-schema contract and the suite's cross-app contract text move with
  the change.

### The door and Warp Station

- The door protocol version moves; `ping` answers it. Existing commands
  (`ping`, `open-workspace`, `new-chat`, `run-prompt`, `cancel`, `models`,
  `status`) keep their shape and behaviour.
- A new `harness` command carries a method name and params; methods mirror the
  trait's app-level operations. The answer is the typed result or a typed error
  (unknown method, disabled harness, unavailable harness, unsupported
  operation).
- A new `subscribe` command opens a stream of harness events, filtered by run,
  session or harness; the door pushes frames with a subscription id and closes
  or reports overflow explicitly. `run-prompt` remains the one-shot convenience.
- Harness metadata lists enabled harnesses with `available` and `capabilities`;
  `status` reports the same availability. Disabled harnesses are not offered,
  and `new-chat`/`run-prompt`/`harness` reject them.
- The door token remains the only gate: no dangerous-command switch and no
  per-client scopes. Auto-allow is unchanged and unrelated; it governs agent
  permission prompts, not door commands.
- Warp Station's client gains the `harness` and `subscribe` calls. Its AI card
  keeps the harness picker (now registry-fed), models and variants, and new
  chat/run prompt; it shows the permission/form card for a run it started when
  auto-allow did not answer it. It does not grow inbox/revert/shell/MCP full
  UIs.
- Both apps land in the same change window. The door framing and version
  constants live in the shared kit, so the kit tag and both app pins move
  together.

### UI

- The composer pill becomes a menu over enabled harnesses; it locks after the
  first session; the label is the harness display name.
- Settings shows a registry-driven harness list: enable switch, executable,
  default selection, and the attached v2 service state (running, stopped and
  started, unavailable, CLI missing, wrong credentials) with a retry.
- v2-only surfaces are capability-gated: forms/question card, inbox panel,
  revert controls, shell input, background-session indicator, MCP management and
  skill activation. Missing capabilities hide the surface rather than showing a
  dead control.
- Warp Station's AI card shows the registry's enabled harnesses, the harness's
  models and variants, and the blocking permission/form card for its own runs.
- Motion, accessible names and the design-system rules apply to every new
  surface; a running harness state is a word plus a spinner, and reduced motion
  keeps the word.

### Contracts and documentation

- The ADRs for the compiled-in registry, opencode v2 as its own harness, the
  attached connection, the auto-start rule, the door namespace and the
  token-only gate are the architectural record.
- The settings-schema contract, the suite's cross-app contract, and both apps'
  agent instructions (harness sections, door change rule) move with the code.
- The suite pin table names the new kit tag and both app tags in the same
  release window.

## Testing Decisions

Good tests assert external behaviour at a seam, not adapter internals. The
feature has three seams.

1. **The `Harness` trait.** This is the primary seam. A shared conformance suite
   runs against every adapter through the trait, with wire-level fakes behind
   each adapter: a fake opencode v1 HTTP/SSE server, a fake opencode v2 HTTP/SSE
   server plus a fake registration file and CLI output, and the existing fake
   `dsh web` server. The suite covers start/stop, capability reporting,
   `Unsupported` errors, catalogs, session lifecycle, prompt streaming,
   permissions, questions/forms, commands/skills, diffs, subagents, v2-only
   operations, error mapping and disable/re-enable. A `FakeHarness` implements
   the same trait for the store and UI: tests drive scripted events and assert
   store state without network or processes. The store's existing probe helpers
   and the real-window probes are the prior art; the transcript stays
   part-shaped, so the existing transcript tests continue to apply.
2. **The door NDJSON protocol.** The cross-app contract is tested over a real
   loopback socket, not shared Rust types. On the warp-agent side: a real door
   with a real store and a fake harness, covering the existing commands, the
   `harness` namespace, `subscribe` frames, error envelopes, token rejection and
   protocol version. On the Warp Station side: its client against the same fake
   door or a scripted NDJSON server, covering harness metadata, models, new
   chat, run prompt, permission/form cards and cancellation. The existing door
   tests and the Station client's tests are the prior art.
3. **The settings documents.** Versioned JSON is the migration boundary: old
   executable fields and default harness become the harness map,
   `opencode-v2` starts disabled, unknown keys survive, the session-to-harness
   values stay, and the model cache keeps its `(harness, model)` identity. The
   existing settings-migration tests and a scratch `HOME` are the prior art.

Supporting tests: the v2 adapter's discovery and auth path (status output,
registration file, 401 re-read, service start, missing CLI, unavailable
service), the v2 event mapping (delta accumulation, permission/form shapes,
subagent child sessions), and the Station run-loop permission/form handling.
Both apps' existing probes must stay green. The suite gates are
`cargo fmt --all --check`, clippy with `-D warnings` on the touched crates, and
`cargo test` for warp-agent, warp-station and warp-station-core, plus the kit
crates if the door constants move.

## Out of Scope

- Runtime or third-party plugin loading; harnesses are compiled-in adapters
  behind the registry.
- Pairing, remote clients and the pairing UI; local discovery through the
  registration file is the path.
- Experimental v2 operations other than skill activation and MCP management
  (session import/export, jobs, filesystem, persistent PTY, automations and the
  rest).
- Full v2 UI parity in Warp Station; it stays a run-loop client.
- Migrating existing sessions from one harness to another; a chat keeps the
  harness it was created on.
- Changing auto-allow semantics or adding a door-level dangerous-command switch.
- A managed private `opencode serve` mode for v2; the adapter attaches to the
  shared service.
- Stopping or restarting the v2 shared service; warp-agent may start it but
  never stops it.
- Secrets management beyond reading the v2 registration file (no OS keyring, no
  password in settings).
- The v2 plugin, PTY, VCS, debug and other non-app surfaces not listed in the
  interface.

## Further Notes

- The architectural decisions are recorded in ADR 0002 (compiled-in harness
  support), ADR 0003 (opencode v2 is its own harness), ADR 0004 (attached shared
  service), ADR 0005 (may start the service), ADR 0006 (the door's harness
  namespace) and ADR 0007 (the door token is the only gate). The
  grill-with-docs session that produced them is the source of the trade-offs;
  this spec does not re-litigate them.
- The change spans three repositories' worth of release pins: the shared kit's
  door protocol, warp-agent and Warp Station. Both apps pin the same new kit
  tag, and the suite pin table names both app tags in the same release window.
- v2's `/api` surface is still labelled experimental in its own documentation.
  The adapter should depend only on the stable app-level operations listed here,
  keep the experimental surface behind capability flags, and record the pinned
  v2 version it was verified against.
- The v2 registration file is channel-dependent off the default channel and uses
  the platform state directory, not a hardcoded Unix path; a missing or stale
  file is the unavailable state, not a crash.
- v1 and v2 share OpenCode's configuration locations; running both is a manual
  coexistence with separate ports and processes, and the app must not rewrite
  either one's config.
- The door remains a trusted-sibling interface: the token is per launch and
  0600, but any process running as the same user can read it. That is a recorded
  trade-off (ADR 0007), not an oversight.
- Ticket breakdown and ordering (expand-contract around the interface, then the
  v2 adapter, then the v2-only surfaces, then Station) and the release window
  are the next step.
