import { cardByName, openCatalog, waitForPlateViewer } from '../lib/app.js';
import { byText, click, reveal } from '../lib/ui.js';

describe('6 Plattenauswahl', () => {
  const group = () => $('[data-plate-selector]');
  const all = () => group().$('button[aria-pressed]');
  const previous = () => group().$('button[aria-label="Vorherige Platte"]');
  const next = () => group().$('button[aria-label="Nächste Platte"]');
  const plate = (n: number) => group().$(`button[aria-label^="Platte ${n}"]`);
  const selected = async (n: number) => {
    await browser.waitUntil(async () => await plate(n).getAttribute('aria-current') === 'true');
    expect(await all().getAttribute('aria-pressed')).toBe('false');
  };

  before(async function () {
    await openCatalog(1280, 800, { pinnedDetails: true });
    for (let attempt = 1; attempt <= 3; attempt++) {
      const card = await cardByName('Dreiplatten.3mf');
      await reveal(card);
      await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
        .move({ origin: card }).down().up().pause(60).down().up().perform();
      if (await $('h1=Dreiplatten.3mf').waitForDisplayed({ timeout: 8000 }).catch(() => false)) break;
      if (attempt === 3) throw new Error('Double click did not open Dreiplatten.3mf');
    }
    await click((await byText('button', '3D-Ansicht'))!);
    if (!await waitForPlateViewer()) {
      console.warn('SKIP: PlateSelector only renders when the 3D viewer is ready; WebGL is unavailable on this runner.');
      this.skip();
    }
  });

  it('shows All, three numbered plates and two arrows below the 3D surface', async () => {
    const g = await browser.execute(() => ({
      barTop: document.querySelector('[data-plate-selector]')!.getBoundingClientRect().top,
      surfaceBottom: document.querySelector('[data-detail-viewer] [data-viewer-surface]')!.getBoundingClientRect().bottom,
    }));
    expect(g.barTop).toBeGreaterThanOrEqual(g.surfaceBottom - 1);
    expect((await group().$$('button').length)).toBe(6);
    expect(await all().getAttribute('aria-pressed')).toBe('true');
    expect(await previous().getAttribute('aria-disabled')).toBe('true');
    expect(await next().getAttribute('aria-disabled')).toBe('false');
    for (const n of [1, 2, 3]) expect(await plate(n).isDisplayed()).toBe(true);
  });

  it('selects plates with the next arrow and numbered buttons', async () => {
    await click(next());
    await selected(1);
    expect(await previous().getAttribute('aria-disabled')).toBe('true');
    await click(plate(2));
    await selected(2);
    expect(await previous().getAttribute('aria-disabled')).toBe('false');
  });

  it('uses Left/Right, keeps the boundary selection and returns to All', async () => {
    await browser.keys(['ArrowRight']);
    await selected(3);
    expect(await next().getAttribute('aria-disabled')).toBe('true');
    await browser.keys(['ArrowRight']);
    await selected(3);
    await browser.keys(['ArrowLeft']);
    await selected(2);
    await click(previous());
    await selected(1);
    await browser.keys(['ArrowLeft']);
    await selected(1);
    await click(all());
    expect(await all().getAttribute('aria-pressed')).toBe('true');
    expect(await group().$('[aria-current="true"]').isExisting()).toBe(false);
  });
});
