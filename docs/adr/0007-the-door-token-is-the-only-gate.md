# The door token is the only gate

_Status: accepted._

A door client that holds the token may call every harness operation the door
exposes, including `shell.run`, `session.revert.*`, `session.delete` and MCP
management; there is no second `door_allow_dangerous` switch. The token is
generated per launch, written 0600 under `~/.warp/agent`, and the door is
loopback-only, so the trust boundary is "a process running as the same user."
The deciding factor is that Warp Station must be able to drive every operation
in the same change window, and a second gate would make the door's behaviour
depend on a setting both apps must surface before the first command runs.

Considered options: a default-off dangerous-command setting (cheap safety, but
two apps must agree on the flag and the first-run flow), and per-client token
scopes (stronger isolation, but a credential-management project of its own).

Consequences: a local process that can read the token can run a shell in a
workspace or revert a session; the door is a trusted-sibling interface, not a
sandbox. The app's auto-allow settings are unchanged and unrelated: they govern
the agent's permission prompts, not door commands.
