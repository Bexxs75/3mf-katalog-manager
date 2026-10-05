import { openCatalog, T } from '../lib/app.js';

describe('2 Katalog laden', () => {
  before(async () => { await openCatalog(); });

  it('shows one card per catalog model and the file counter', async () => {
    await browser.waitUntil(async () => (await $$('[data-model-id]').length) === 8, { timeoutMsg: 'expected 8 cards' });
    const text = await $('header').getText();
    expect(text).toContain(T.filesCount(8));
    expect(await $('body').getText()).not.toContain('Dein Katalog ist noch leer');
  });

  it('lists the seeded model names', async () => {
    // textContent, not getText(): WebKitWebDriver reports "" for text that sits in an ellipsis-clipped box.
    const names = await browser.execute(() => Array.from(document.querySelectorAll('[data-model-id] [data-card-metadata] > div:first-child')).map((e) => (e.textContent ?? '').trim()));
    expect(names.sort()).toEqual(['Dreiplatten.3mf', 'Farbwuerfel.3mf', 'Kegel.stl', 'Platte 40mm.stl', 'Prisma.stl', 'Pyramide.stl', 'Würfel 20mm.stl', 'Zylinder.stl']);
  });
});
