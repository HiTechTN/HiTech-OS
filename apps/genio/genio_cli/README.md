# genio_cli — TUI/text client (Phase 2)

Thin client for the Genio daemon. Holds **no** privileges and **no** secrets:
it authenticates to the daemon (Unix socket `/run/genio/genio.sock`, or
`127.0.0.1:8765` in dev) and renders the agent's state and proposals.

Planned Phase 2 scope: session REPL, live telemetry pane, approval prompts for
R2 actions, `genio ask "..."`, `genio status`.
