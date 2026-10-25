# Desktop GUI

`wfgui` is Warframe's Rust desktop client.

```bash
retinue daemon start
wfgui
wfcli gui install
```

`wfcli gui install` registers the desktop launcher and app icon. Use
`wfcli gui status` or `wfcli gui uninstall` to inspect or remove it.

- Foundry: equipment, ownership, components, favorites and crafting filters.
- Mastery Helper: progress and candidates from owned equipment, relics or platinum.
- Inventory: parts, sets, relics, mods and arcanes with prices and filters.
- Relic Planner: owned relics, reward search and refinement values.
- Market: account, listings and order management.
- Build Planner: equipment configurations, Overframe builds, groups and Forma plans.
- Settings: daemon status and image-cache controls.

The right rail shows timers/fissures or a market order book. Click a component to
view listings, its void badge to find relics, or right-click for related actions.
The title bar provides scaling and animated rail controls. Collapsed navigation
keeps the player's mastery emblem; hover it for the full name and rank.
Favorites and UI scale are saved between launches. `--ui-scale 125` overrides
the saved scale for one run; changing it in the title bar saves the new value.

The preserved `wfgui-qt` executable remains available for migration comparisons.

## Captures

```bash
wfgui --screenshot previews/foundry.png --size 2560x1440 --ui-scale 125
wfgui --screenshot previews/card.png --capture-target foundry.card --search Hildryn
wfgui --screenshot previews/filters.png --capture-target foundry.filters --ui-scale 125
wfgui --screenshot previews/component.png --capture-target foundry.component --search "Ash Prime"
wfgui --fixture --screenshot previews/settings.png --page settings
wfgui --page builds --select "Regulators Prime" --hover build:mod:mod-1 \
  --capture-target build.mod-preview --screenshot previews/mod.png
```

Captures use live daemon data unless `--fixture` is supplied. Captures and fixtures
ignore saved preferences and never change them; their default scale is 100%.
`--window-screenshot` captures an actual desktop frame and exits. Additional targets and tests are in
[native UI development](developer/native-ui.md); build commands are in
[workflows](developer/workflows.md).
