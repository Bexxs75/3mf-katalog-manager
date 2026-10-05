import { cardByName, closeSettings, openCatalog, openSettings, railTo, setViewport, T } from '../lib/app.js';
import { expectNoProblems, runAudit } from '../lib/audit.js';
import { byText, click, waitForText } from '../lib/ui.js';

// One test per (view, width) so a failure names the exact combination.
const WIDTHS = [900, 1000, 1280];
const HEADER_MAX = 60; // header is h-[54px]; a wrapped second line would exceed this

type View = { name: string; open: () => Promise<void>; close?: () => Promise<void> };

const views: View[] = [
  { name: 'Katalog', open: async () => { await railTo('Katalog'); await $('[data-model-id]').waitForExist(); } },
  { name: 'Printer Manager', open: async () => { await railTo('Printer Manager'); await $('h1=Testdrucker Alpha').waitForDisplayed(); } },
  { name: 'Material Manager', open: async () => { await railTo('Material Manager'); await waitForText('div', 'Material Manager'); } },
  ...(['Allgemein', 'Katalog', 'Info'] as const).map((tab): View => ({
    name: `Einstellungen ${tab}`,
    open: async () => { await openSettings(tab); await browser.pause(300); },
    close: closeSettings,
  })),
  ...(['Bild', '3D-Ansicht'] as const).map((mode): View => ({
    name: `Detailseite (${mode})`,
    open: async () => {
      await railTo('Katalog');
      const card = await cardByName('Dreiplatten.3mf');
      await browser.action('pointer', { parameters: { pointerType: 'mouse' } }).move({ origin: card }).down().up().pause(60).down().up().perform();
      await $('h1=Dreiplatten.3mf').waitForDisplayed({ timeout: 15000 });
      await click((await byText('button', mode))!);
      if (mode === '3D-Ansicht') await $('[role="radiogroup"][aria-label="Druckplatte"]').waitForDisplayed({ timeout: 60000 });
    },
    close: async () => { await click($('button[title="Zurück zum Katalog"]')); },
  })),
];

describe('7 Layout-Audit', () => {
  before(async () => { await openCatalog(1280); });

  // Proves the audit can fail: without this, "all green" could also mean "checks never match anything".
  it('self-check: reports a deliberately truncated label and a header overflow', async () => {
    await setViewport(1280);
    await railTo('Katalog');
    await browser.execute(() => {
      const el = document.createElement('div');
      el.id = 'mfk-selftest';
      el.textContent = 'Eine absichtlich viel zu lange Beschriftung';
      el.style.cssText = 'position:fixed;top:120px;left:120px;width:60px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;z-index:99999';
      document.body.appendChild(el);
      const wide = document.createElement('div');
      wide.id = 'mfk-selftest-wide';
      wide.style.cssText = 'position:absolute;top:0;left:0;width:3000px;height:1px';
      document.documentElement.appendChild(wide);
    });
    const problems = await runAudit('layout', { maxHeaderHeight: HEADER_MAX, helpLabel: T.helpLabel });
    await browser.execute(() => { document.getElementById('mfk-selftest')?.remove(); document.getElementById('mfk-selftest-wide')?.remove(); });
    const checks = problems.map((p) => p.check);
    expect(checks).toContain('ellipsis');
    expect(checks).toContain('document-scroll-x');
  });

  it('self-check: reports a header that does not fit and a plate bar on top of the 3D view', async () => {
    // Header: push the help icon out of the window by inserting a wide fixed-width item (the window cannot go below 900 px).
    await setViewport(900);
    await railTo('Katalog');
    await browser.execute(() => {
      const spacer = document.createElement('div');
      spacer.id = 'mfk-selftest-spacer';
      spacer.style.cssText = 'flex:none;width:900px;height:10px';
      document.querySelector('header')!.prepend(spacer);
    });
    const narrow = await runAudit('layout', { maxHeaderHeight: HEADER_MAX, helpLabel: T.helpLabel });
    await browser.execute(() => document.getElementById('mfk-selftest-spacer')?.remove());
    await setViewport(1280);
    expect(narrow.map((p) => p.check)).toContain('help-outside');
    // Plate bar: pull the real radio group over the 3D surface.
    await views.find((v) => v.name === 'Detailseite (3D-Ansicht)')!.open();
    await browser.execute(() => {
      const bar = document.querySelector<HTMLElement>('[role="radiogroup"][aria-label="Druckplatte"]')!;
      const surface = document.querySelector<HTMLElement>('[data-detail-viewer] [data-viewer-surface]')!;
      const r = surface.getBoundingClientRect();
      bar.style.cssText = `position:fixed;left:${r.left + 10}px;top:${r.top + 10}px;z-index:99999`;
    });
    const covered = await runAudit('layout', { maxHeaderHeight: HEADER_MAX, helpLabel: T.helpLabel });
    await views.find((v) => v.name === 'Detailseite (3D-Ansicht)')!.close!();
    expect(covered.map((p) => p.check)).toContain('plate-bar-over-3d');
  });

  for (const view of views) {
    for (const width of WIDTHS) {
      it(`${view.name} bei ${width} px`, async () => {
        await setViewport(width);
        await view.open();
        await browser.pause(300); // layout settles after the last render
        const problems = await runAudit('layout', { maxHeaderHeight: HEADER_MAX, helpLabel: T.helpLabel });
        await view.close?.();
        await expectNoProblems(problems, `layout ${view.name} ${width}`);
      });
    }
  }
});
