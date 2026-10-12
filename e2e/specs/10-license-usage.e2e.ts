import { cardByName, openCatalog } from '../lib/app.js';
import { button, chooseOption, closeDetails, detailPanel, selectCards, selectModel } from '../lib/catalog-ui.js';
import { click, waitForText } from '../lib/ui.js';

describe('10 Lizenz und Nutzung', () => {
  const names = ['Würfel 20mm.stl', 'Zylinder.stl'];
  const usage = () => $('select[aria-label="Nutzung"]');
  before(async () => { await openCatalog(1280, 800, { pinnedDetails: true }); });

  it('saves a private license and filters private, commercial and all models', async () => {
    await selectModel(names[0]);
    const license = detailPanel().$('section[aria-label="Lizenz"]');
    await license.waitForDisplayed();
    await chooseOption(license.$('select'), 'CC BY-NC – Namensnennung, nicht kommerziell');
    await button('Lizenz speichern');
    await browser.waitUntil(async () => await (await cardByName(names[0])).getAttribute('aria-label') === `${names[0]}, Nur privat`);
    expect(await license.getText()).toContain('Nur privat');
    expect(await (await cardByName(names[0])).getAttribute('aria-label')).toBe(`${names[0]}, Nur privat`);
    expect(await (await cardByName(names[0])).$('span.status-warn').getText()).toBe('Nur privat');
    await closeDetails();
    await chooseOption(usage(), 'Nur privat');
    await waitForText('span', '1 von 8');
    expect((await $$('[data-model-id]').length)).toBe(1);
    await chooseOption(usage(), 'Kommerziell erlaubt');
    await waitForText('span', '0 von 8');
    await waitForText('h3', 'Kein Modell passt zu diesen Filtern');
    expect((await $$('[data-model-id]').length)).toBe(0);
    await chooseOption(usage(), 'Alle');
    await browser.waitUntil(async () => (await $$('[data-model-id]').length) === 8);
  });

  it('sets CC BY for two selected cards and removes their badges with Nicht gesetzt', async () => {
    await selectCards(names);
    for (const label of ['CC BY – Namensnennung', 'Nicht gesetzt']) {
      await button('Lizenz setzen');
      const editor = $('[data-navigation-menu] section[aria-label="Lizenz"]');
      await editor.waitForDisplayed();
      await chooseOption(editor.$('select'), label);
      await click(editor.$('button'));
      await editor.waitForExist({ reverse: true });
      for (const name of names) {
        const card = await cardByName(name);
        if (label === 'Nicht gesetzt') {
          expect(await card.getAttribute('aria-label')).toBe(name);
          expect(await card.$('span.status-good, span.status-warn').isExisting()).toBe(false);
        } else {
          expect(await card.getAttribute('aria-label')).toBe(`${name}, Kommerziell erlaubt`);
          expect(await card.$('span.status-good').getText()).toBe('Kommerziell erlaubt');
        }
      }
    }
  });
});
