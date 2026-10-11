# MCP Server

`retinue warframe mcp` gives MCP hosts structured access to Warframe services in the persistent
`retinue-daemon`. `wfcli mcp` selects the same adapter. The host starts one short-lived stdio process;
that process connects to or starts the shared daemon.

## Register

Build the desired environment, then register its `retinue` executable:

```bash
make prod-erlang
codex mcp add retinue -- /absolute/path/to/retinue/prod/bin/retinue warframe mcp
```

Development registration can use `dev/bin/retinue warframe mcp`. Standard output contains only
newline-delimited JSON-RPC messages; diagnostics use standard error.

## Tools

The Warframe adapter provides:

- `query`: runs the shared query language. Results are unlimited unless `limit` is supplied.
- `forma_plan`: queues the daemon-owned planner and returns canonical data.
- `daemon_status`: starts the daemon if needed and returns queue, cache, protocol, and build state.
- `update_knowledge`: refreshes selected daemon-managed metadata and WFCD caches.

`retinue mcp` instead exposes `daemon_status`, `poe2_session`, and the Warframe tools prefixed
with `warframe_`. Choose the focused adapter when the host only needs Warframe.

Tool calls are monitor-owned. MCP cancellation or host exit terminates the adapter worker;
`retinue_client` then releases the corresponding queued or running daemon request.

## Resources

- `wfcli://datasets`: query datasets and descriptions.
- `wfcli://query-language`: compact grammar and examples.
- `wfcli://schema/worldstate`: normalized fields and type-specific views.
- `wfcli://daemon/status`: live daemon status as JSON.

The CLI owns MCP framing and JSON conversion. Fetching, persistence, query evaluation, and Forma
planning remain in `retinue-daemon`. The stdio process opens no network listener; Erlang distribution is
bound to loopback and protected by the normal distribution cookie.
