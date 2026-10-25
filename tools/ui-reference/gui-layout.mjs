const qt = "apps/wfgui/qt/src/";

// Qt values describe source geometry, not measurements of a running Qt window.
export const contracts = {
  foundry: [
    ["Card", "foundry.card", ".foundryObject", "player_item_grid_widget.cpp", { intentional: "Compact native/Qt card; no Aleca footer" }],
    ["Component", "foundry.part", ".foundryObjectComponentsComponent", "player_item_grid_widget.cpp", { diameter: 39 }],
  ],
  mastery: [
    ["Game content", "mastery.summary.game", ".masteryHelperMidRect", "mastery_planner_widget.cpp", { minimumWidth: 290, distribution: "space-around" }, 0],
    ["Star Chart", "mastery.summary.star", ".masteryHelperMidRect", "mastery_planner_widget.cpp", { minimumWidth: 252 }, 1],
    ["Intrinsics", "mastery.summary.intrinsics", ".masteryHelperMidRect", "mastery_planner_widget.cpp", { minimumWidth: 180 }, 2],
    ["Card", "page.card", ".masteryHelperItem", "player_item_grid_widget.cpp", { trackHeight: 91, cardInset: 4, radius: 12 }],
    ["Artwork", "mastery.card.image", ".masteryHelperItemImage", "player_item_grid_widget.cpp", { width: 72, height: 72 }],
    ["Component", "mastery.card.component", ".foundryObjectComponentsComponent", "player_item_grid_widget.cpp", { diameter: 30 }],
  ],
  relics: [
    ["Card", "page.card", ".inventoryObject", "relic_card_layout.cpp", { trackHeight: 108, cardInset: 4 }],
    ["Title", "relic.card.title", ".inventoryItemName", "relic_grid_widget.cpp", { fontSize: 19, weight: 500 }],
    ["Refinement", "relic.card.refinement", ".relicDetailsBLTopExpected", "relic_card_layout.cpp", { height: 18, fourRowsHeight: 16 }],
    ["Reward", "relic.card.reward", ".foundryObjectComponentsComponent", "relic_card_layout.cpp", { diameter: 37, gap: 4, columns: 3 }],
  ],
  inventory: [
    ["Card", "page.card", ".inventoryObject", "player_item_grid_widget.cpp", { trackHeight: 130, cardInset: 4, radius: 20 }],
    ["Artwork", "inventory.card.image", ".inventoryItemImage", "inventory_card_layout.cpp", { columnWidth: 90 }],
    ["Title", "inventory.card.title", ".inventoryItemName", "inventory_card_layout.cpp", { height: 27, alignment: "center" }],
    ["Component", "inventory.card.component", ".foundryObjectComponentsComponent", "inventory_card_layout.cpp", { diameter: 35, gap: 6 }],
  ],
  market: [["Card", "page.card", null, "market_order_card.cpp", { height: 107, headerHeight: 28, artwork: 44 }]],
  builds: [["Tabs", null, null, "build_planner_widget.cpp", { order: ["Groups", "Equipment", "Discover"], shape: "capsule", warning: "triangle" }]],
  settings: [["Sections", null, null, "settings_widget.cpp", { groups: ["Daemon", "Storage"] }]],
};

export function compareLayout(page, native, reference) {
  const layout = native.layout;
  return {
    units: "logical_pixels", page, scale: layout?.scale,
    caveat: "Different data and intentional UI changes are not automatic parity failures. Qt is source-derived; native and Aleca are measured.",
    components: (contracts[page] ?? []).map(([name, region, selector, file, geometry, occurrence]) => ({
      name, qt: { source: qt + file, ...geometry },
      native: region ? { region, bounds: layout?.regions?.[region] ?? null,
        elements: (layout?.elements ?? []).filter(e => {
          const r = layout.regions?.[region], b = e.bounds;
          return r && b && b.x >= r.x-1 && b.y >= r.y-1 && b.x+b.width <= r.x+r.width+1 && b.y+b.height <= r.y+r.height+1;
        }) } : null,
      aleca: selector ? { selector, occurrence,
        elements: reference?.elements?.[selector]?.slice(occurrence ?? 0, occurrence === undefined ? 6 : occurrence + 1) ?? [] } : null,
    })),
    controls: layout?.controls ?? [],
  };
}
