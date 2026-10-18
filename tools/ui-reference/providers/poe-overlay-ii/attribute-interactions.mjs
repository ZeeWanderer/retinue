import assert from "node:assert/strict";

export default async function exercise(page) {
  const report = [];
  const row = text => page.locator('[data-type="explicit"] > div').filter({ hasText: text });
  const life = row("+80 to maximum Life");
  const fire = row("+35% to Fire Resistance");
  const cold = row("+28% to Cold Resistance");
  const snapshot = async name => {
    const state = await page.evaluate(() => ({
      inputs: [...document.querySelectorAll('input[type="number"]')].map(input => ({
        value: input.value, placeholder: input.placeholder, disabled: input.disabled,
        focused: input === document.activeElement,
      })),
      selected: [...document.querySelectorAll('[data-included]')].map(e => e.textContent),
    }));
    report.push({ name, ...state });
  };
  const clickInputArea = async (field) => {
    const rect = await field.boundingBox();
    assert.ok(rect);
    await page.mouse.click(rect.x + rect.width / 2, rect.y + rect.height / 2);
  };
  await page.waitForFunction(() => document.querySelectorAll('input[type="number"]').length >= 6);
  await snapshot("initial");
  assert.equal(await life.locator('input').first().isDisabled(), true);
  await clickInputArea(life.locator('input').first());
  await page.waitForTimeout(100);
  await snapshot("click-excluded-input");
  assert.equal(await life.locator('input').first().isDisabled(), true);
  const min = await life.locator('input').first().boundingBox();
  const max = await life.locator('input').last().boundingBox();
  await page.mouse.click((min.x + min.width + max.x) / 2, min.y + min.height / 2);
  await page.waitForTimeout(100);
  await snapshot("click-input-wrapper");
  assert.equal(await life.locator('input').first().isDisabled(), false);
  assert.equal(await life.locator('input').first().evaluate(e => e === document.activeElement), false);
  await life.locator('input').first().click();
  await page.keyboard.type("9");
  assert.equal(await life.locator('input').first().inputValue(), "9");
  await snapshot("focus-selects-text");
  await cold.getByText("+28% to Cold Resistance", { exact: true }).click();
  await life.locator('input').last().click();
  await page.keyboard.press("Tab");
  assert.equal(await cold.locator('input').first().evaluate(e => e === document.activeElement), true);
  assert.equal(await fire.locator('input').first().isDisabled(), true);
  await snapshot("tab-skips-excluded");
  await page.keyboard.press("Shift+Tab");
  assert.equal(await life.locator('input').last().evaluate(e => e === document.activeElement), true);
  await snapshot("backtab-skips-excluded");
  await life.getByText("+80 to maximum Life", { exact: true }).click();
  assert.equal(await life.locator('input').first().isDisabled(), true);
  assert.equal(await life.locator('input').first().getAttribute("placeholder"), "9");
  await life.getByText("+80 to maximum Life", { exact: true }).click();
  assert.equal(await life.locator('input').first().inputValue(), "9");
  await snapshot("toggle-preserves-bound");
  return report;
}
