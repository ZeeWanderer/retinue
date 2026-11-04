# Rendering Reference: 2026-11-03

Initial post-migration reference, not a comparison with the old SDK or Qt.
Clean commit `cb0a0ce` was built in a separate worktree and compared with the
uncommitted PoE sorting, league-menu and shared damage-preparation changes.
Exact executable, source, toolchain and fixture identities are in each record.

## Collection

- Six maintained suites, 73 cases; 12 fresh processes per build and suite, ABBA order.
- Optimized test binaries; no compilation during measurements or live app changes.
- i7-10700, performance governor, benchmark affinity CPUs 4-7: four physical cores.
- Four rendering threads for GUI/PoE; two for Warframe. Counts include the caller.
- GUI scales 100/125/200%; PoE also 150%. Warframe includes 1080p/1440p/2160p.
- Fixed fixtures, fresh XDG directories per process, normal OS file cache. No game.
- Native completion is timed. Compositor, network and live artwork costs are excluded.

## Results

Exploratory per-case p95 tolerance remains `max(0.1 ms, 5%)`, with 95% intervals
over independent process summaries. **58 pass, 15 inconclusive, no confirmed
regression.** This is not an all-passing acceptance result or a calibrated CI gate.

| Suite | Reference | Working Tree | Pass / Inconclusive |
| --- | --- | --- | ---: |
| Rails | [Record](records/2ce8be93eaf54b648805092a139ed346.json) | [Record](records/0bc7d8128a4a431e914ffbfbaa9f8511.json) | 1 / 3 |
| Foundry scrolling | [Record](records/e76a8c75f523457abcab5965c573b0e7.json) | [Record](records/50abd83711834b34b1576ff4701e44d4.json) | 6 / 2 |
| Foundry dragging | [Record](records/c6d00fcf5ec44941a59db5dcdf77fe5e.json) | [Record](records/217cf613426d4c308f3fcc932c76e1de.json) | 1 / 5 |
| Relics scrolling/dragging | [Record](records/bf6eac47ed024e999cca316e09f78616.json) | [Record](records/bc127b4969bf44249c6497d29be5adcc.json) | 6 / 0 |
| Warframe overlay | [Record](records/e7b5a19659114cf1aaa4e64f4cd5e69a.json) | [Record](records/94cb825817784c009f28074b9f0ad4d4.json) | 6 / 3 |
| PoE overlay | [Record](records/04504b566cfd4c169f6066ea19b0c2d4.json) | [Record](records/1652cbd25cd047e68362cec915c6e218.json) | 38 / 2 |

At 1440p/100%, current mean-of-run medians are 2.84 ms for Foundry dragging
(2.96 ms over the grid), 2.48 ms for Relics wheel scrolling and 3.37 ms for Relics
dragging. Drag traces dispatch eight pointer events per frame. The old severe
Foundry drag-over-grid slowdown did not reproduce in this fixture scenario.

## Repeatability

The initial unpinned eight-run comparison had 56 passing and 17 inconclusive
cases. An eight-run identical-binary control had 45 passing and 28 inconclusive
cases: uncertainty exists without a source change. Neither reported a confirmed
regression. Median/mean screens were also checked separately, not substituted for
the reported tail screen; the final pinned comparison had 65 pass / 8 inconclusive
for each of those metrics.

Pinning the identical-binary rail control made all four cases pass at the original
tolerance; the full fixed-affinity comparison still had uncertain tails. Pinning
is a measurement control, not a proven runtime optimization or complete isolation.
Do not widen limits merely to turn these results green.

The six clean-source records are selected as advisory historical references.
Small regressions, the earlier Relics input overhead and live frame pacing remain
unresolved; selecting a reference does not declare those issues fixed. No renderer
or cache-policy change was justified by this comparison alone.
