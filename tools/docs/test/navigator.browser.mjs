import assert from 'node:assert/strict';
import test from 'node:test';
import {chromium} from 'playwright-core';

test('guides and declarations share a navigable collection on a direct visit', async () => {
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage();
    const base = process.env.DOCS_URL ?? 'http://127.0.0.1:4321/retinue/';
    await page.goto(new URL('developer/companion/', base).href);
    const rail = page.getByRole('navigation', {name:'Documentation contents'});
    const roots = () => rail.locator(':scope > ul > li > a, :scope > ul > li > details > summary, :scope > ul > .outline-marker').allTextContents();
    const initial = await roots();
    for (const [name, target] of [
      ['InventoryChanges', 'developer/player-data/#inventory-snapshots-and-deltas'],
      ['Element','reference/rust/retinue_ui/scene/struct.Element/'],
      ['Canvas','reference/rust/retinue_ui/drawing/pixels/struct.PixelCanvas/'],
      ['Recording','reference/rust/retinue_ui/drawing/layers/struct.PixelLayers/'],
      ['wl_shm::Format::Argb8888','#variant.Argb8888'],
    ]) {
      const link = page.locator('main').getByRole('link',{name, exact:true}).first();
      assert.ok((await link.getAttribute('href')).endsWith(target), name);
    }
    const daemon = rail.locator('summary').getByRole('link',{name:'retinue_daemon', exact:true});
    await daemon.locator('..').click({position:{x:4,y:9}});
    await rail.getByRole('link',{name:'Daemon lifecycle'}).waitFor({state:'visible'});
    await daemon.click();
    await rail.getByRole('link',{name:'Daemon lifecycle'}).click();
    assert.deepEqual(await roots(), initial);
    assert.equal(await rail.locator('[aria-current="page"]').textContent(), 'Daemon lifecycle');
    const collection = daemon.locator('xpath=../..');
    assert.ok(await collection.getByText('Warframe / Market', {exact:true}).isVisible());
    await rail.getByRole('link',{name:'wf_market_service',exact:true}).click();
    assert.ok(await rail.getByRole('link',{name:'Daemon lifecycle'}).isVisible());
    assert.deepEqual(await roots(), initial);
  } finally {
    await browser.close();
  }
});
