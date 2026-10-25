import assert from "node:assert/strict";

const read = page => page.evaluate(() => {
  const track = [...document.querySelectorAll("div")].find(element => {
    const style = getComputedStyle(element);
    return style.height === "3px" && style.backgroundColor === "rgb(11, 11, 11)";
  });
  if (!track) return null;
  const fill = track.firstElementChild;
  const style = getComputedStyle(fill);
  return {
    height: track.getBoundingClientRect().height,
    fraction: fill.getBoundingClientRect().width / track.getBoundingClientRect().width,
    alignment: getComputedStyle(track).justifyContent,
    color: style.backgroundColor,
    duration: style.transitionDuration,
    easing: style.transitionTimingFunction,
  };
});

export default async function exercise(page) {
  await page.waitForTimeout(400);
  const full = await read(page);
  assert.ok(full, "run with --state cooldown");
  assert.equal(full.alignment, "flex-end");
  assert.equal(full.fraction, 1);
  await page.clock.setFixedTime(new Date("2026-10-13T10:00:05Z"));
  await page.waitForTimeout(900);
  const half = await read(page);
  assert.ok(Math.abs(half.fraction - 0.5) < 0.001);
  assert.equal(half.alignment, "flex-end");
  await page.clock.setFixedTime(new Date("2026-10-13T10:00:11Z"));
  await page.waitForTimeout(900);
  assert.equal(await read(page), null);
  return {full, half, expired: "hidden"};
}
