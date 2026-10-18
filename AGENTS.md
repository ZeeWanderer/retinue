# AGENTS

## Workflow
- Be concise. Keep user-facing docs standalone.
- Keep changes scoped; commit only when requested, with title-only messages.
- Prefer clear code; comment only non-obvious reasoning.
- Save large outputs to files. Inspect selected fields or bounded excerpts.

## Architecture
- Follow [project structure](docs/developer/structure.md). Share mechanisms, not game-specific policy.
- Keep fetching, canonical data and domain computation in the daemon; presentation and input in clients.
- Update affected apps and tests in lockstep; follow [compatibility policy](docs/developer/daemon.md#build-identity).

## Build And Test
- Use Make targets from [workflows](docs/developer/workflows.md). Rebuild the selected
  dev/prod flavor before manual checks; do not edit staged files.
- GUI configuration refreshes automatically. Do not default to `make clean` after toolchain updates.
- Verify the running binary and reload completion. Preserve the user's running game and services.
- Code, fixture and build changes require `./scripts/test-quiet ct` plus affected-layer tests.
  Documentation-only edits need no tests. Add regression coverage; keep fixtures read-only.
- Socket and Erlang distribution tests need host namespaces, not necessarily root.
- Exercise changed CLI commands. Inspect UI details and scaling with [GUI captures](docs/gui.md)
  and [overlay previews](tools/ui-reference/README.md).
- Distinguish automated checks from live verification. Ask for the game when needed; report remaining gaps.

## Game Integrations
- Use validated anchors and bounded reads, not per-build offset tables.
  Test relocation, ambiguity and structural changes.
- Failed reads are not empty state. Use existing [health reporting](docs/developer/health.md);
  untested adapters remain unverified, even when a fallback succeeds.
