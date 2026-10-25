export const providers = {
  alecaframe: {
    desktop: "desktop.mjs",
    foundry: "foundry.mjs",
    "relic-rewards": "rewards.mjs",
    "relic-suggestions": "suggestions.mjs",
    "relic-planner": "planner.mjs",
    "market-orders": "market.mjs",
  },
  "poe-overlay-ii": {
    evaluate: "evaluate.mjs",
  },
};

export function renderer(provider, scene) {
  const scenes = Object.hasOwn(providers, provider) && providers[provider];
  if (!scenes) throw new Error(`Unknown provider: ${provider}`);
  if (!Object.hasOwn(scenes, scene)) throw new Error(`Unknown ${provider} scene: ${scene}`);
  return new URL(`providers/${provider}/${scenes[scene]}`, import.meta.url);
}
