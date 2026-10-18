# AlecaFrame Overlay Catalog

Snapshot: AlecaFrame 2.6.90, released June 30, 2026. Evidence comes from
official documentation and the matching packaged client prepared under ignored
`research/alecaframe/2.6.90/`.

| Surface | Local status |
| --- | --- |
| Relic rewards | Implemented |
| Relic recommendations | Implemented |
| Chat Riven analysis | Researched |
| Riven reroll comparison | Researched |
| Trade completion | Researched; needs authenticated Market support |
| TennoFinder notification | Researched |

## Relic Rewards

Opens when a fissure presents reward choices.

Each card shows:

- item name, platinum value, and highest-value highlight;
- ducats;
- vaulted and favourite markers;
- crafted or mastered state;
- owned count and recipe requirement;
- other set components, their owned counts, and set value.

Footer may show account platinum and ducats. AlecaFrame can copy recognized
rewards and prices to clipboard.

Detection starts from Warframe `OutputDebugString` received through DBWIN.
`Got rewards` delays capture for UI stabilization, crops reward labels, runs
OCR, fuzzy-matches items, and batches item and set prices. Requiem rewards are
skipped by this snapshot.

The reference renderer records exact Flexbox/Grid structure, computed styles,
and content-dependent element positions. Setup and rendering commands are in
[`tools/ui-reference/README.md`](../../tools/ui-reference/README.md).

## Relic Recommendations

Opens with the fissure relic chooser. It displays owned relics for the detected
era using filters and ordering from AlecaFrame's desktop relic planner.

Cards show:

- relic name, owned count, vaulted state, and favourite state;
- expected platinum and ducats;
- void traces and refresh progress.

`ThemedProjectionManager.lua: LoadingCompleteEnd` triggers a small era crop.
OCR identifies Lith, Meso, Neo, Axi, Requiem, or All. Recommendations come from
cached inventory and reward data; visible relic tiles are not individually
OCRed.

## Chat Riven Analysis

Opens after the user follows a Riven link in game chat. It shows:

- weapon and Riven names;
- positive and negative stats;
- per-stat roll quality and overall grade;
- similar Rivens with similarity, price, source, and links;
- known good positives and acceptable negatives.

DBWIN debug output identifies the detailed item dialog. AlecaFrame captures and
OCRs the Riven card. Interaction begins through the Overwolf hotkey.

## Riven Reroll Comparison

Opens while cycling a Riven. Left panel retains the old roll; right panel shows
the new roll. Both use the chat Riven stats, grades, comparisons, and price
model.

Selection, confirmation, completion, and close signals come from DBWIN debug output.
Before and after cards are OCRed. Both modes use the same packaged
`RivenOverlay` window.

## Trade Completion

Package-only interactive prompt. After a recognized trade, AlecaFrame matches
the other player and item against the connected Warframe Market account. It
can decrement or close a regular order, close a Riven auction, and submit
positive reputation. A countdown applies the default close action.

This requires Market authentication, current orders, and trade text parsed from
DBWIN debug output.

## TennoFinder Notification

Package-only six-second notification used by AlecaFrame's separate squad tool.
It handles joins, leaves, kicks, disbands, and messages, with optional sound.
It is positioned on the right edge near 80% of game height.

## Desktop Features

These are not contextual game overlays:

- Foundry, Inventory, Mastery Helper, Relic Planner, Riven Explorer, Stats,
  Trading Analytics, Market account management, and build tools;
- Windows and Discord notifications for world state, Market messages, and
  Riven sniper matches;
- main AlecaFrame, build, and TennoFinder windows.

## Market Account Features

AlecaFrame can:

- view and edit regular orders and Riven auctions;
- create buy or sell orders from inventory and set views;
- change price, quantity, visibility, and order state;
- copy ready-to-paste Market whispers;
- receive Market messages and online status;
- reconcile completed trades and leave reputation.

Public price lookup does not require an account. Authenticated order mutation
is a separate daemon capability with owner-only credentials and explicit user
actions.

## Evidence

Package files:

- `manifest.json`: in-game windows and desktop exclusions
- `web/relicOverlay.html`, `assets/js/relicOverlay/main.js`: reward UI
- `web/relicRecommendation.html`: recommendation UI
- `web/rivenOverlay.html`: Riven analysis and comparison
- `web/tradeFinishedNotification.html`: trade prompt
- `web/InGameNotification.html`: generic notification
- `Utils/EELogProcessor.cs`: triggers and close events
- `Utils/OverlaysHandler.cs`: era OCR and recommendation startup
- `OCRHelper.cs`: reward capture and enrichment
- `Data/RivenOverlays.cs`: Riven modes
- `Data/WFMarketHelper.cs`: trade matching

Public references:

- [Overlay overview](https://docs.alecaframe.com/overlays/overview)
- [Relic rewards](https://docs.alecaframe.com/overlays/relic-rewards)
- [Relic recommendations](https://docs.alecaframe.com/overlays/relic-recommendation)
- [Chat Riven](https://docs.alecaframe.com/overlays/riven-chat)
- [Riven reroll](https://docs.alecaframe.com/overlays/riven-reroll)
- [Warframe Market](https://docs.alecaframe.com/features/wfm)
- [Overwolf package page](https://www.overwolf.com/app/alejandro_cabrerizo-alecaframe)

Implementation ownership and safety constraints are in
[`companion.md`](companion.md) and
[`overwolf-alecaframe-overlay-research.md`](overwolf-alecaframe-overlay-research.md).
