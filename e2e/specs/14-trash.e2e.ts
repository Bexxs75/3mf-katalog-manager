import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { cardByName, openCatalog, railTo } from '../lib/app.js';
import { button, catalogCount } from '../lib/catalog-ui.js';
import { click, rightClick, waitForText } from '../lib/ui.js';

describe('14 Papierkorb', () => {
  before(async () => { await openCatalog(); });

  it('deletes through the context menu and restores the catalog entry and original file', async () => {
    const name = 'Würfel 20mm.stl';
    const path = join(process.env.MFK_E2E_CATALOG!, 'Deko', name);
    const original = readFileSync(path);
    await rightClick(await cardByName(name));
    // Removing only the catalog entry keeps the file; Delete exercises the physical trash.
    await button('Löschen');
    await waitForText('button', 'Abbrechen');
    await button('Löschen');
    await catalogCount(7);
    await browser.waitUntil(async () => !existsSync(path));
    await browser.waitUntil(async () => (await $('[aria-label="Papierkorb"]').getText()).trim() === '1');
    await railTo('Papierkorb');
    await waitForText('h1', 'Papierkorb');
    const card = await cardByName(name);
    expect((await $$('[data-model-id]').length)).toBe(1);
    await click(card);
    await button('Wiederherstellen');
    await waitForText('p', 'Papierkorb ist leer.');
    await browser.waitUntil(async () => existsSync(path));
    expect(readFileSync(path).equals(original)).toBe(true);
    expect((await $('[aria-label="Papierkorb"]').getText()).trim()).toBe('');
    await railTo('Katalog');
    await catalogCount(8);
    await cardByName(name);
  });
});
