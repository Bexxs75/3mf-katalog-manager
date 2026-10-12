import { cardByName, closeSettings, openCatalog, openSettings, railTo, setViewport, T, waitForPlateViewer, railToAndWait } from '../lib/app.js';
import { expectNoProblems, runAudit } from '../lib/audit.js';
import { byText, click, waitForText } from '../lib/ui.js';

// One test per (view, width) so a failure names the exact combination.
const WIDTHS = [900, 1000, 1280];
const HEADER_MAX = 60; // header is h-[54px]; a wrapped second line would exceed this

type View = { name: string; open: () => Promise<void>; close?: () => Promise<void> };

const views: View[] = [
  { name: 'Katalog', open: async () => { await railToAndWait('Katalog', async () => await $('[data-model-id]').isExisting()); } },
  { name: 'Printer Manager', open: async () => { await railToAndWait('Printer Manager', async () => await $('h1=Testdrucker Alpha').isDisplayed()); } },
  { name: 'Material Manager', open: async () => { await railToAndWait('Material Manager', async () => (await byText('div', 'Material Manager')) !== null); } },
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
      if (mode === '3D-Ansicht') await waitForPlateViewer();
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

  it('self-check: ignores screenreader text but still reports visibly clipped text', async () => {
    await browser.execute(() => {
      const host = document.createElement('div');
      host.id = 'mfk-selftest-hidden';
      host.style.cssText = 'position:fixed;top:150px;left:150px;z-index:99999';
      const add = (text: string, css: string, className = '') => {
        const label = document.createElement('span');
        label.className = className;
        label.textContent = text;
        label.style.cssText = css;
        const child = document.createElement('span');
        child.textContent = text;
        label.appendChild(child);
        host.appendChild(label);
      };
      add('audit sr-only label', '', 'sr-only');
      add('audit clipped pixel label', 'position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0)');
      add('audit clip-path label', 'position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%)');
      const visible = document.createElement('div');
      visible.textContent = 'audit visible clipped label';
      visible.style.cssText = 'width:30px;overflow:hidden;white-space:nowrap';
      host.appendChild(visible);
      document.body.appendChild(host);
    });
    try {
      const problems = await runAudit('layout', { maxHeaderHeight: HEADER_MAX, helpLabel: T.helpLabel });
      expect(problems.some(p => p.text === 'audit visible clipped label' && p.check === 'clipped')).toBe(true);
      expect(problems.filter(p => p.text.startsWith('audit ') && p.text !== 'audit visible clipped label')).toEqual([]);
    } finally {
      await browser.execute(() => document.getElementById('mfk-selftest-hidden')?.remove());
    }
  });

  it('self-check: reports a header that does not fit', async () => {
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
  });

  it('self-check: reports a plate bar on top of the 3D view', async function () {
    await views.find((v) => v.name === 'Detailseite (3D-Ansicht)')!.open();
    if (!await $('[data-plate-selector]').isDisplayed()) {
      await views.find((v) => v.name === 'Detailseite (3D-Ansicht)')!.close!();
      console.warn('SKIP: the plate-overlap self-check requires WebGL; the app reports it unavailable.');
      this.skip();
    }
    await browser.execute(() => {
      const bar = document.querySelector<HTMLElement>('[data-plate-selector]')!;
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
