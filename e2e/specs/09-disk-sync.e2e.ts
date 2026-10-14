import { copyFileSync, existsSync, mkdirSync, renameSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { cardByName, openCatalog, railTo } from '../lib/app.js';
import { button, catalogCount } from '../lib/catalog-ui.js';
import { click, waitForText } from '../lib/ui.js';

describe('9 Festplatten-Abgleich', () => {
  const catalogDir = process.env.MFK_E2E_CATALOG!;
  const dialog = () => $('[role="dialog"][aria-label="Abgleich mit der Festplatte"]');
  const removal = () => dialog().$('input[type="checkbox"]');
  before(async () => { await openCatalog(); });

  it('does not import a case-only rename again on Windows/macOS', async function () {
    if (process.platform !== 'win32' && process.platform !== 'darwin') {
      console.warn('SKIP: case-only path equivalence is a Windows/macOS contract; Linux paths are case-sensitive.');
      this.skip();
    }
    const original = join(catalogDir, 'Haushalt', 'Kegel.stl');
    const renamed = join(catalogDir, 'Haushalt', 'kegel.stl');
    const temporary = join(catalogDir, 'Haushalt', 'case-rename.tmp');
    renameSync(original, temporary);
    renameSync(temporary, renamed);
    try {
      await button('Aktualisieren');
      await waitForText('div', 'Alles aktuell');
      expect(await dialog().isExisting()).toBe(false);
      await catalogCount(8);
    } finally {
      renameSync(renamed, temporary);
      renameSync(temporary, original);
    }
  });

  it('previews, defers and applies additions while retaining missing entries by default', async () => {
    await button('Aktualisieren');
    await waitForText('div', 'Alles aktuell');
    const addedDir = join(catalogDir, 'sync-neu');
    mkdirSync(addedDir);
    copyFileSync(join(catalogDir, 'Deko', 'Zylinder.stl'), join(addedDir, 'neues-modell.stl'));
    unlinkSync(join(catalogDir, 'Deko', 'Würfel 20mm.stl'));
    await button('Aktualisieren');
    await dialog().waitForDisplayed();
    for (const summary of ['1 neue Datei', '1 neuer Ordner', '1 Datei fehlt']) await waitForText('span', summary);
    await waitForText('label', 'Fehlende Dateien aus dem Katalog entfernen');
    expect(await removal().isSelected()).toBe(false);
    await button('Später');
    await dialog().waitForExist({ reverse: true });
    await catalogCount(8);
    await cardByName('Würfel 20mm.stl');
    expect(await $('[data-model-id][aria-label="neues-modell.stl"]').isExisting()).toBe(false);
    expect(await $('//aside//span[normalize-space()="sync-neu"]').isExisting()).toBe(false);
    expect(existsSync(join(addedDir, 'neues-modell.stl'))).toBe(true);
    expect(existsSync(join(catalogDir, 'Deko', 'Würfel 20mm.stl'))).toBe(false);

    await browser.keys(['F5']);
    await dialog().waitForDisplayed();
    for (const summary of ['1 neue Datei', '1 neuer Ordner', '1 Datei fehlt']) await waitForText('span', summary);
    expect(await removal().isSelected()).toBe(false);
    await button('Änderungen übernehmen');
    await dialog().waitForExist({ reverse: true });
    await catalogCount(9);
    await cardByName('neues-modell.stl');
    await cardByName('Würfel 20mm.stl');
    expect(existsSync(join(addedDir, 'neues-modell.stl'))).toBe(true);
    expect((await $('[aria-label="Papierkorb"]').getText()).trim()).toBe('');
  });

  it('removes the missing entry only after opting in and increments the trash counter', async () => {
    await button('Aktualisieren');
    await dialog().waitForDisplayed();
    for (const summary of ['0 neue Dateien', '0 neue Ordner', '1 Datei fehlt']) await waitForText('span', summary);
    expect(await removal().isSelected()).toBe(false);
    await click(removal());
    expect(await removal().isSelected()).toBe(true);
    await button('Entfernen');
    await dialog().waitForExist({ reverse: true });
    await catalogCount(8);
    expect(await $('[data-model-id][aria-label="Würfel 20mm.stl"]').isExisting()).toBe(false);
    await browser.waitUntil(async () => (await $('[aria-label="Papierkorb"]').getText()).trim() === '1');
    await railTo('Papierkorb');
    await cardByName('Würfel 20mm.stl');
  });
});
