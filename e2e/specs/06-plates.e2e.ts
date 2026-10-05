import { cardByName, openCatalog } from '../lib/app.js';
import { byText, click, sleep } from '../lib/ui.js';

describe('6 Plattenauswahl', () => {
  before(async () => {
    await openCatalog();
    const card = await cardByName('Dreiplatten.3mf');
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: card }).down().up().pause(60).down().up().perform(); // real double click
    await $('h1=Dreiplatten.3mf').waitForDisplayed({ timeout: 15000 });
    // The seeded 3MF has an embedded picture, so the page opens on "Bild"; switch to the 3D view.
    await click((await byText('button', '3D-Ansicht'))!);
  });

  const group = () => $('[role="radiogroup"][aria-label="Druckplatte"]');
  const radios = () => group().$$('[role="radio"]');
  const checked = async () => {
    const out: string[] = [];
    for (const r of await radios()) if ((await r.getAttribute('aria-checked')) === 'true') out.push((await r.getText()).trim());
    return out;
  };

  it('shows the plate bar below the 3D surface', async () => {
    await group().waitForDisplayed({ timeout: 60000, timeoutMsg: 'plate bar never appeared (3D view did not become ready)' });
    const g = await browser.execute(() => {
      const bar = document.querySelector('[role="radiogroup"][aria-label="Druckplatte"]')!.getBoundingClientRect();
      const surface = document.querySelector('[data-detail-viewer] [data-viewer-surface]')!.getBoundingClientRect();
      return { barTop: bar.top, surfaceBottom: surface.bottom };
    });
    expect(g.barTop).toBeGreaterThanOrEqual(g.surfaceBottom - 1);
    expect((await radios()).length).toBe(4);
    expect(await checked()).toEqual(['Alle']);
  });

  it('selects a plate with a real click', async () => {
    const plate2 = (await radios())[2];
    await click(plate2);
    expect(await checked()).toEqual(['Platte 2']);
  });

  it('moves with arrow keys, Home and End and keeps aria-checked in sync', async () => {
    await browser.keys(['ArrowRight']);
    expect(await checked()).toEqual(['Platte 3']);
    await browser.keys(['ArrowRight']); // wraps to "Alle"
    expect(await checked()).toEqual(['Alle']);
    await browser.keys(['End']);
    expect(await checked()).toEqual(['Platte 3']);
    await browser.keys(['Home']);
    expect(await checked()).toEqual(['Alle']);
    await browser.keys(['ArrowLeft']);
    expect(await checked()).toEqual(['Platte 3']);
    // Roving tabindex: only the selected radio is in the tab order.
    const tabbable: string[] = [];
    for (const r of await radios()) if ((await r.getAttribute('tabindex')) === '0') tabbable.push((await r.getText()).trim());
    expect(tabbable).toEqual(['Platte 3']);
    await sleep(0);
  });
});
