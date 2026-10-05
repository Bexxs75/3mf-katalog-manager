import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { cardByName, expandCatalogRoot, openCatalog, sidebarRow } from '../lib/app.js';
import { dragAndDrop } from '../lib/ui.js';

describe('5 Ziehen mit der Maus', () => {
  before(async () => {
    await openCatalog();
    await expandCatalogRoot();
  });

  // The counter is the last <span> in the row.
  const count = async (name: string) => {
    const spans = Array.from(await (await sidebarRow(name)).$$('span'));
    return Number(await spans[spans.length - 1].getText());
  };

  it('moves a card onto a folder: file lands in the target directory and counters update', async () => {
    const catalog = process.env.MFK_E2E_CATALOG!;
    expect(await count('Ablage')).toBe(0);
    expect(await count('Deko')).toBe(2);
    await dragAndDrop(await cardByName('Würfel 20mm.stl'), await sidebarRow('Ablage'));
    await browser.waitUntil(() => existsSync(join(catalog, 'Ablage', 'Würfel 20mm.stl')), { timeoutMsg: 'file did not arrive in Ablage' });
    expect(existsSync(join(catalog, 'Deko', 'Würfel 20mm.stl'))).toBe(false);
    await browser.waitUntil(async () => (await count('Ablage')) === 1 && (await count('Deko')) === 1, { timeoutMsg: 'folder counters did not update' });
  });

  it('adds a card to a collection by dropping it on the collection', async () => {
    await dragAndDrop(await cardByName('Kegel.stl'), await sidebarRow('Testsammlung'));
    await browser.waitUntil(async () => (await $('body').getText()).includes('„Kegel.stl“ zur Sammlung „Testsammlung“ hinzugefügt'), { timeoutMsg: 'no confirmation message' });
    await browser.waitUntil(async () => (await count('Testsammlung')) === 1, { timeoutMsg: 'collection counter did not update' });
  });
});
