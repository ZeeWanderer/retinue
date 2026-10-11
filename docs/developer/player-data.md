# Player Data Acquisition

`retinue-daemon` owns the canonical `player` dataset. `wfcompanion` collects the local account
observation through a narrow read-only native collector and publishes the untouched raw payload.

## AlecaFrame Finding

AlecaFrame does not acquire Warframe inventory through its own C# code. Its background page asks
Overwolf Game Events Provider (GEP) for `inventory` and `match_info`, receives
`match_info.inventory`, and passes the JSON string to `SetWarframeData`.

Evidence in the retained AlecaFrame 2.6.90 research copy:

- `package/web/assets/js/background.js` registers the two GEP features and forwards
  `info.info.match_info.inventory`.
- `csharp/.../OverwolfWrapper.cs::SetWarframeData` validates, parses, and caches the payload.
- `csharp/.../Data/DataHandler.cs::LoadWarframeData` unwraps `InventoryJSON` when present and
  builds lookup tables.
- `csharp/.../Data/Types/WarframeRootObject.cs` describes the observed account payload.

The [official Overwolf Warframe event documentation](https://dev.overwolf.com/ow-electron/live-game-data-gep/supported-games/warframe/)
confirms that `match_info.inventory` contains item types and amounts for the local player. The
sample application only consumes GEP; it does not publish the Warframe-specific collector.

The retained Overwolf `308.0.14` provider identifies the missing collector as
`plugins/64/gep_warframeext.dll`. Its native code:

- locates `Warframe.x64.exe` and opens it with access mask `0x410`, process query plus read-only
  virtual-memory access;
- walks game memory with `VirtualQueryEx` and `ReadProcessMemory`;
- watches an in-memory HTTP-response structure, finds JSON containing `LastInventorySync`, and
  publishes the enclosing value as `game_info.inventory`;
- watches the in-memory debug log for
  `ThemedDetailedPurchaseDialog.lua: PopulateInfo->` and publishes the parsed value as
  `game_info.highlighted`.

The provider imports no `WriteProcessMemory` or socket/HTTP APIs. This supports an external
read-only memory scanner rather than packet interception. Reproducible decompiler output,
hashes, and the untouched provider package are retained under `research/overwolf/`.

`apps/wfcompanion/src/inventory.rs` reproduces only the inventory path from the Linux host. It
opens `/proc/<game-pid>/mem` read-only, finds the HTTP manager through the same masked executable
signature, and semantically decodes the response handler to derive version-dependent field
offsets. Every 7 ms it compares the cached payload length plus its terminator. A detected change
reads the provider's `0x9e1fff`-byte response capacity in 256 KiB chunks until the string terminator;
indirect and alternate responses are then checked in provider order. Published payloads must contain
a complete inventory object with `LastInventorySync`.

Launch mode makes `wfcompanion` the game process's ancestor, satisfying normal Linux ptrace
restrictions without debugger attachment or elevated privileges. The collector does not inject,
write game memory, intercept networking, or collect Overwolf's separate highlighted-item event.
It remains version-sensitive; failure must disable player enrichment without affecting
worldstate, CLI, or overlay operation.

## Payload Semantics

The account payload contains more than stack counts. AlecaFrame derives its views by joining these
records to its static item catalog by internal `ItemType` or unique name.

- Owned equipment comes from arrays such as `Suits`, `LongGuns`, `Pistols`, `Melee`, companions,
  Archwing equipment, modular equipment, and their equivalents.
- Stack inventory comes mainly from `MiscItems`, `Recipes`, `RawUpgrades`, `Upgrades`, relics,
  resources, and consumables. `ItemCount` is the owned quantity.
- Mastery progress comes from `XPInfo`, equipment-specific XP, `PlayerLevel`, completed normal and
  Steel Path nodes in `Missions`, junction completion, and Railjack or Duviri ranks in
  `PlayerSkills`.
- Foundry state comes from `PendingRecipes`; component ownership is derived by joining recipe
  components to stack inventory.
- Other useful state includes focus, affiliations, currencies, slots, challenges, quests,
  scans, intrinsics, Archon shards, and account timestamps such as `LastInventorySync`.

Owned and mastered are separate facts. An item can be absent from current inventory but remain
mastered through `XPInfo`. Collection completion is likewise derived: enumerate supported catalog
items, then join current ownership, historical mastery, pending builds, and component quantities.

Unknown payload fields must survive ingestion. Warframe adds account fields independently of this
application, so a strict decoder that drops unknown data would make the `player` query surface less
useful and force updates for unrelated additions.

## Ownership

Collector process:

- Acquires account payload with the read-only Overwolf-compatible native collector.
- Publishes one versioned raw observation over the local companion socket.
- Does not calculate collection percentages or persist the canonical snapshot.

`retinue-daemon`:

- Stores the raw source namespace in `player.term` with owner-only permissions.
- Parses known fields while preserving the raw map.
- Joins account records to daemon-owned item and recipe catalogs.
- Produces inventory, equipment, mastery, collection, foundry, and summary entities.
- Publishes one revision so every local client sees the same state.

Consumers:

- `wfcli` formats and queries player entities.
- `wfcompanion` uses only fields needed by contextual overlays.
- Graphical clients own navigation and view state, not account persistence or derivation.

Raw account data must stay on the local authenticated socket. Do not send it over Erlang
distribution, include it in incident logs, or upload it without explicit informed opt-in.

## Current Contract

Companion observation schema `2` contains collector metadata, collection time, game PID, sync
value, profile, and raw payload. Schema `1` snapshots with a companion-built index remain readable
for cache migration.

`retinue-daemon` persists the latest observation in `player.term` with owner-only permissions. Player
projection derives equipment, item stacks, ranked upgrades, configs, loadouts, mastery, recipes,
missions, skills, affiliations, focus state, boosters, and challenges. Raw and typed records share
origins; query defaults to typed records for covered origins and raw records elsewhere. Raw
fields remain available through `data.*`, normalized nested fields through `typed.*`.

`wf_player_schema` reports unknown fields and shape changes in understood records without
rejecting production payloads. The player fixture must audit cleanly so schema drift becomes a
test failure when fixtures are refreshed.

`wf_player_views` joins that snapshot to the managed WFCD item catalog. It derives inventory
categories and sets plus mastery ownership, rank, pending builds, missing components, owned-relic
availability, and completion summaries. GUI filtering remains client-owned.
