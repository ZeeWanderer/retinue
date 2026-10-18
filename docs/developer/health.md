# Health Evidence

`retinue doctor` is a read-only consumer. Game reporters own the meaning of each
check; `retinue_checks` owns shared deduplication, recovery and bounded persistence.
It is independent of player datasets and game service supervision. Raw rotating
logs remain supporting event history, not a source from which to guess recovery.
Doctor combines live observations with unresolved ledger entries; session resets
must not hide a previous failure. Historical failures are labelled as recorded
evidence, not proof that the current build reproduced them. Store errors must not
produce a clean report. Filter unresolved history by `last_result: open`, not the
version-dependent view state; an upgrade does not reopen resolved incidents.

## Reporters

Rust clients use `retinue-client::health::Health`, one instance per game session.
Record the final result of existing work under a fixed check ID. Successful
fallbacks do not verify a failed adapter. Do not create checks per item, address
or error message. Session replacement resets live evidence.

Recording updates a bounded in-memory map (64 IDs); it performs no I/O. Publish
only first evidence and result transitions, using the atomic revision to avoid
serializing unchanged snapshots. The snapshot preserves the last failure even
when recovery replaces a pending publication. On-demand diagnostics expose fresh
attempt times without continuous health traffic. Do not add polling or expensive
identity discovery for reporting.

Socket clients declare interface `checks: 1` and publish:

```json
{"op":"publish","id":10,"dataset":"checks","data":{"context":{"session":"process:start:generation","game_build":"sha256"},"checks":{"native_inventory":{"state":"fail","checked_at":1790000000000,"error":"anchor not found"}}}}
```

Success uses `state: "pass"` and `error: null`. Optional `last_failure` contains
`checked_at` and `error`, preserving failure followed by recovery in one snapshot.
The daemon takes component and Retinue version from the handshake, not payload
claims. Game build and session describe the observation, not a guessed current
build. Erlang services use `retinue_checks:observe/3`; PoE2 feeds this from existing
session observations, without a second sampler or publication stream.

## Storage

Healthy-only evidence remains in memory. A failure arms durable history for its
component/check key. Explicit success resolves it. Repeated success in the same
context causes no write; changed durable evidence is batched with a five-second
write interval. Repeated failures count received observations, not every local
attempt. Replays and older timestamps are ignored. Retinue upgrades without new
evidence yield `unverified`, not automatic recovery.

The private JSON store uses atomic replacement, a bounded read, 512 records and
30-day resolved retention. Capacity pressure evicts resolved entries first;
`evicted_since_start` exposes overflow. Shutdown flushes pending changes. Storage
errors remain visible. The ledger is the last root supervisor child so its
failure cannot restart game workers.

Keep occurrence time separate from receipt time and preserve source version,
following the [OpenTelemetry log data model](https://opentelemetry.io/docs/specs/otel/logs/data-model/).
No telemetry backend or exporter is required. Tests must cover untested state,
recovery, replay, session changes, healthy-path write suppression and malformed
storage. A daemon restart does not turn cached evidence into live health.
