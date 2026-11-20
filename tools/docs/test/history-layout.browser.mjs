import assert from 'node:assert/strict';
import test from 'node:test';
import {chromium} from 'playwright-core';

test('measurement summaries fit the pane and session details survive resizing', async () => {
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({viewport: {width: 1920, height: 900}});
    await page.goto(new URL('research/benchmarks/', process.env.DOCS_URL ?? 'http://127.0.0.1:4321/retinue/').href);
    const toggle = page.getByRole('button', {name: /^Details for session /}).first();
    await toggle.waitFor();
    const detail = page.locator('#' + await toggle.getAttribute('aria-controls'));
    await toggle.focus();
    await page.keyboard.press('Enter');
    for (const width of [1920, 1440, 1140, 1024, 768, 390, 320]) {
      await page.setViewportSize({width, height: 900});
      await page.waitForFunction(() => {
        const plot = document.querySelector('.plot');
        return Number(plot.querySelector('svg')?.getAttribute('width')) === Math.round(plot.getBoundingClientRect().width);
      });
      assert.equal(await toggle.getAttribute('aria-expanded'), 'true');
      assert.ok(await detail.isVisible());
      assert.equal(await detail.getByRole('link').count(), 2);
      assert.ok(await detail.getByText('Middle 50%', {exact: true}).isVisible());
      const dimensions = await page.evaluate(() => {
        const pane = document.querySelector('.main-pane');
        const scroll = document.querySelector('.measurements');
        const row = document.querySelector('tbody tr');
        return {gutter: pane.clientWidth - scroll.clientWidth, overflow: scroll.scrollWidth - scroll.clientWidth,
          pageOverflow: document.documentElement.scrollWidth - innerWidth, rowHeight: row.offsetHeight};
      });
      assert.ok(dimensions.gutter <= 64, 'unused pane at ' + width);
      assert.ok(dimensions.overflow <= 1 && dimensions.pageOverflow <= 1, JSON.stringify({width, ...dimensions}));
      assert.ok(dimensions.rowHeight < 90, 'squeezed session row at ' + width);
    }
    await page.keyboard.press('Space');
    assert.ok(await detail.isHidden());
  } finally {
    await browser.close();
  }
});
