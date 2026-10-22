# genio_web — web interface (Phase 4)

Browser client (SPA) for the Genio daemon: dashboards (telemetry), agent
conversation with SSE streaming, and the human approval workflow for privileged
(R2) actions.

Security notes: the daemon API is same-origin only; privileged actions require
an explicit signed intent from an authenticated admin; extracted document HTML
is rendered exclusively inside a sandboxed iframe.
