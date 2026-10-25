# Qt Reference GUI

This documents the retained `wfgui-qt` client while its pages move to Rust.
Qt source is in `apps/wfgui/qt/`; normal builds do not compile it.
It starts or reuses `retinue-daemon` and
releases its activity lease when the window closes.

![Foundry](../images/wfgui-foundry.png)

Build instructions and staged executable names are in [development workflows](workflows.md#build).

Install the application launcher and notification identity:

```bash
wfcli gui install
```

Use `wfcli gui status` or `wfcli gui uninstall` to inspect or remove it.

## Pages

### Foundry

Foundry joins player inventory, pending recipes, mastery state, and catalog metadata. Search and
item-type filters run locally. Cards show ownership, mastery, favourite and vault state, component
requirements, and available quantities.

### Mastery Helper

Mastery Helper combines equipment, historical mastery XP, pending builds, components, and owned
relics. Its header shows progress within the current Mastery Rank. Filters isolate unfinished items
by available progress source. Hovering a component shows its name and owned/required count.

### Inventory

Inventory covers parts, sets, relics, mods, arcanes, and miscellaneous stacks. Search, category,
ownership, and sorting controls are local. Visible assets are prioritized over cards outside the
viewport.

### Relic Planner

Relic Planner ranks relics by expected four-player platinum value, then ducat value. It supports
era and name filters, owned-only display, price refresh, vault state, owned counts, and Void Trace
count. Hovering a reward shows its name.

Each era keeps independent state. Metadata appears first; prices and images update in place.
Changing era never displays another era's results while data is pending.

### Market

Market signs in to Warframe Market, manages regular buy and sell orders, and controls account
presence. Orders can be filtered, edited, hidden, closed, deleted, or reconciled with the current
player inventory. The open page refreshes account data once per minute.

Click a tradable item, component, relic, or relic reward on another page to open current listings
in the WFMarket rail. When the rail is collapsed, the same view opens as a dialog. It supports
exact rank, charge, star, subtype, and bulk-trade variants, copies trade whispers, and can create
an order for a signed-in account.

The password is used only for sign-in. `retinue-daemon` stores the returned session token in its
owner-only state directory. Presence can follow Warframe automatically or remain online, in game,
or invisible.

### Settings

Settings shows daemon connection state and source-image, rendered-image, and in-memory cache usage.
Each cache can be cleared independently; downloaded source images are fetched again when needed.

### Build Planner

Equipment shows each owned copy and its saved configurations. Discover browses Overframe builds;
adding a build keeps the current page open, with an explicit **Open group** action.

Groups combine target builds for one owned copy. **Calculate** shows required polarity changes,
ordered swaps and Forma applications, then each build's resulting mod layout and
capacity. Select a member and switch between its target build and planned loadout. Notes follow
the cards in the same scroll area.

New groups allow rearrangement while preserving weapon elemental combinations. **Keep exact mod
slots** locks positions; unresolved elemental effects require this option. Companion precept and Parazon
order stay fixed. New Omni and Umbral Forma applications are separate planning options.

## Activity Rail

The right rail has Timers & Events and WFMarket tabs. Timers shows active world cycles, Baro and
Prime Resurgence windows, and grouped fissures. Its fissure bell cycles between notifications off,
active while a GUI is connected, and persistent daemon monitoring. `wfcli notifications` controls
the same setting.

WFMarket searches the item catalog, shows public WTS or WTB listings, copies trade whispers, and
posts account orders without leaving the current page.

## Screenshots

Render a deterministic window image without a desktop screenshot tool:

```bash
wfgui-qt --screenshot /tmp/wfgui.png --size 2560x1440 --page foundry
wfgui-qt --market-item 54a74454e779892d5e515621 --market-side sell \
  --screenshot /tmp/wfgui-market-item.png
wfgui-qt --activity-tab market --screenshot /tmp/wfgui-market-rail.png
```

Valid pages are `foundry`, `mastery`, `inventory`, `relic`, `market`, and `settings`.

List and capture smaller UI regions at native resolution:

```bash
wfgui-qt --list-capture-targets
wfgui-qt --market-item 54a74454e779892d5e515621 \
  --capture market-item.quantity --capture-padding 12 \
  --screenshot /tmp/wfgui-quantity.png
wfgui-qt --page inventory --capture inventory.grid.item \
  --screenshot /tmp/wfgui-inventory-card.png
```

Targets ending in `.item` select the first visible cell in that grid or list. Use `--size` to
evaluate the target under a specific window layout.
