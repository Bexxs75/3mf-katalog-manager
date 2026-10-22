import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { cardByName, openCatalog } from '../lib/app.js';
import { byText, click, rightClick } from '../lib/ui.js';

describe('3 Rechtsklick-Menue und Umbenennen', () => {
  before(async () => { await openCatalog(); });

  it('opens the model menu on a real right click', async () => {
    await rightClick(await cardByName('Würfel 20mm.stl'));
    for (const entry of ['In Slicer öffnen', 'Im Dateimanager anzeigen', 'Umbenennen', 'Aus dem Katalog entfernen', 'Löschen']) {
      // Outside the selection bar, which has its own "Löschen" button.
      await $(`//button[normalize-space()="${entry}" and not(ancestor::*[@role="toolbar"])]`).waitForExist({ timeout: 5000, timeoutMsg: `no menu entry "${entry}"` });
    }
  });

  it('renames the card and the file on disk using only the keyboard for the text', async () => {
    await click((await byText('button', 'Umbenennen'))!);
    const input = await $('input[type="text"], input:not([type])');
    await input.waitForDisplayed();
    await browser.keys(['Control', 'a']);
    // Lower case on purpose: WebKitWebDriver drops the shift state for upper-case letters sent as plain keys.
    await browser.keys('neuer wuerfel'.split(''));
    await browser.keys(['Enter']);
    await cardByName('neuer wuerfel.stl');
    const catalog = process.env.MFK_E2E_CATALOG!;
    expect(existsSync(join(catalog, 'Deko', 'neuer wuerfel.stl'))).toBe(true);
    expect(existsSync(join(catalog, 'Deko', 'Würfel 20mm.stl'))).toBe(false);
  });
});
