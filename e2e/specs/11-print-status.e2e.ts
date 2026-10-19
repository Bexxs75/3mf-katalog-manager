import { cardByName, openCatalog } from '../lib/app.js';
import { button, chooseOption, detailPanel, selectModel } from '../lib/catalog-ui.js';
import { click, waitForText } from '../lib/ui.js';

describe('11 Druckstatus', () => {
  const name = 'Würfel 20mm.stl';
  const filter = () => $('select[aria-label="Druckstatus"]');
  before(async () => { await openCatalog(1280, 800, { pinnedDetails: true }); });

  it('marks one model printed, filters both states and clears chips individually and together', async () => {
    await selectModel(name);
    await button('Als gedruckt markieren');
    await waitForText('button', 'Als nicht gedruckt markieren');
    await chooseOption(filter(), 'Gedruckt');
    await waitForText('span', '1 von 8');
    await cardByName(name);
    expect((await $$('[data-model-id]').length)).toBe(1);
    await click($('button[aria-label="Gedruckt entfernen"]'));
    await browser.waitUntil(async () => (await $$('[data-model-id]').length) === 8);
    expect(await filter().getValue()).toBe('');
    await chooseOption(filter(), 'Noch nicht gedruckt');
    await waitForText('span', '7 von 8');
    expect((await $$('[data-model-id]').length)).toBe(7);
    expect(await $(`[data-model-id][aria-label="${name}"]`).isExisting()).toBe(false);
    expect(await $('button[aria-label="Noch nicht gedruckt entfernen"]').isDisplayed()).toBe(true);
    await button('Filter zurücksetzen');
    await browser.waitUntil(async () => (await $$('[data-model-id]').length) === 8);
    expect(await filter().getValue()).toBe('');
    expect(await $('button[aria-label="Noch nicht gedruckt entfernen"]').isExisting()).toBe(false);
  });

  it('sorts unprinted models first and the printed model last by default', async () => {
    await click($('header button[aria-haspopup="menu"]:has(span.truncate)'));
    await button('Druckstatus');
    const direction = await waitForText('button', 'Noch nicht gedruckte zuerst');
    expect(await direction.getAttribute('aria-checked')).toBe('true');
    await browser.keys(['Escape']);
    const expected = ['Dreiplatten.3mf', 'Farbwuerfel.3mf', 'Kegel.stl', 'Platte 40mm.stl', 'Prisma.stl', 'Pyramide.stl', 'Zylinder.stl', name];
    await browser.waitUntil(async () => {
      const names = await browser.execute(() => Array.from(document.querySelectorAll('[data-model-id] [data-card-metadata] > div:first-child')).map(el => el.textContent?.trim()));
      return JSON.stringify(names) === JSON.stringify(expected);
    }, { timeoutMsg: 'print-status order must put the seven unprinted models before Würfel 20mm.stl' });
  });
});
