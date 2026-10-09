import { cardByName, openCatalog } from '../lib/app.js';
import { byText, click } from '../lib/ui.js';

describe('6 Plattenauswahl', () => {
  before(async () => {
    await openCatalog();
    const card = await cardByName('Dreiplatten.3mf');
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: card }).down().up().pause(60).down().up().perform();
    await $('h1=Dreiplatten.3mf').waitForDisplayed({ timeout: 15000 });
    // The fixture opens on its embedded picture, so switch to the 3D view.
    await click((await byText('button', '3D-Ansicht'))!);
  });

  const group = () => $('[data-plate-selector]');
  const trigger = () => group().$('button[aria-haspopup="listbox"]');
  const all = () => group().$('button[aria-pressed]');
  const previous = () => group().$('button[aria-label="Vorherige Platte"]');
  const next = () => group().$('button[aria-label="Nächste Platte"]');
  const list = () => group().$('[role="listbox"]');
  const activeNumber = async () => browser.execute(() => {
    const box = document.querySelector('[data-plate-selector] [role="listbox"]')!;
    return document.getElementById(box.getAttribute('aria-activedescendant')!)?.querySelector('span')?.textContent;
  });

  it('shows four controls below the 3D surface with All pressed', async () => {
    await group().waitForDisplayed({ timeout: 60000, timeoutMsg: 'plate bar never appeared (3D view did not become ready)' });
    const g = await browser.execute(() => {
      const bar = document.querySelector('[data-plate-selector]')!.getBoundingClientRect();
      const surface = document.querySelector('[data-detail-viewer] [data-viewer-surface]')!.getBoundingClientRect();
      return { barTop: bar.top, surfaceBottom: surface.bottom };
    });
    expect(g.barTop).toBeGreaterThanOrEqual(g.surfaceBottom - 1);
    expect((await group().$$('button')).length).toBe(4);
    expect(await all().getAttribute('aria-pressed')).toBe('true');
    expect(await previous().isEnabled()).toBe(false);
    expect(await trigger().getAttribute('aria-expanded')).toBe('false');
  });

  it('selects with the next arrow and a list option', async () => {
    await click(next());
    expect(await trigger().getAttribute('title')).toMatch(/^Platte 1(?: ·|$)/);
    expect(await previous().isEnabled()).toBe(false);
    expect(await all().getAttribute('aria-pressed')).toBe('false');
    await click(trigger());
    expect(await trigger().getAttribute('aria-expanded')).toBe('true');
    const bounds = await browser.execute(() => {
      const popup = document.querySelector('[data-plate-selector] [role="listbox"]')!.getBoundingClientRect();
      const viewer = document.querySelector('[data-detail-viewer]')!.getBoundingClientRect();
      return { contained: popup.top >= viewer.top && popup.bottom <= viewer.bottom && popup.left >= viewer.left && popup.right <= viewer.right,
        height: popup.height, scrollWidth: document.documentElement.scrollWidth, viewport: innerWidth };
    });
    expect(bounds.contained).toBe(true);
    expect(bounds.height).toBeLessThanOrEqual(240);
    expect(bounds.scrollWidth).toBeLessThanOrEqual(bounds.viewport);
    const options = await list().$$('[role="option"]');
    expect(options.length).toBe(3);
    expect(await options[0].getAttribute('aria-selected')).toBe('true');
    await click(options[1]);
    expect(await trigger().getAttribute('title')).toMatch(/^Platte 2(?: ·|$)/);
    expect(await trigger().getAttribute('aria-expanded')).toBe('false');
  });

  it('navigates active rows with arrows, Home and End, selects with Enter and dismisses with Escape', async () => {
    await browser.keys(['ArrowDown']);
    expect(await trigger().getAttribute('aria-expanded')).toBe('true');
    expect(await activeNumber()).toBe('Platte 2');
    await browser.keys(['ArrowDown']);
    expect(await activeNumber()).toBe('Platte 3');
    await browser.keys(['ArrowUp']);
    expect(await activeNumber()).toBe('Platte 2');
    await browser.keys(['Home']);
    expect(await activeNumber()).toBe('Platte 1');
    await browser.keys(['End']);
    expect(await activeNumber()).toBe('Platte 3');
    await browser.keys(['ArrowRight']);
    expect(await $('h1=Dreiplatten.3mf').isDisplayed()).toBe(true);
    await browser.keys(['Enter']);
    expect(await trigger().getAttribute('title')).toMatch(/^Platte 3(?: ·|$)/);
    expect(await next().isEnabled()).toBe(false);
    await browser.keys(['ArrowUp']);
    expect(await list().$('[aria-selected="true"]').$('span').getText()).toBe('Platte 3');
    await browser.keys(['Escape']);
    expect(await trigger().getAttribute('aria-expanded')).toBe('false');
    expect(await trigger().isFocused()).toBe(true);
    await click(all());
    expect(await all().getAttribute('aria-pressed')).toBe('true');
  });
});
