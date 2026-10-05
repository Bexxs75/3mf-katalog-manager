import { cardByName, closeSettings, openCatalog, openSettings, railTo } from '../lib/app.js';
import { expectNoProblems, runAudit } from '../lib/audit.js';

// Runtime counterpart of src/styles/typography.test.js: that one reads the
// source, this one asks the browser which font really got applied and which
// characters really reached the screen.
const views: { name: string; open: () => Promise<void>; close?: () => Promise<void> }[] = [
  { name: 'Katalog', open: async () => { await railTo('Katalog'); await $('[data-model-id]').waitForExist(); } },
  { name: 'Printer Manager', open: async () => { await railTo('Printer Manager'); await $('h1=Testdrucker Alpha').waitForDisplayed(); } },
  { name: 'Material Manager', open: async () => { await railTo('Material Manager'); await browser.pause(400); } },
  { name: 'Papierkorb', open: async () => { await railTo('Papierkorb'); await browser.pause(400); } },
  ...(['Allgemein', 'Slicer', 'Katalog', 'Info'] as const).map((tab) => ({
    name: `Einstellungen ${tab}`, open: async () => { await openSettings(tab); await browser.pause(300); }, close: closeSettings,
  })),
  { name: 'Kontextmenue', open: async () => {
      await railTo('Katalog');
      const card = await cardByName('Kegel.stl');
      await browser.action('pointer', { parameters: { pointerType: 'mouse' } }).move({ origin: card }).down({ button: 2 }).up({ button: 2 }).perform();
      await browser.pause(300);
    }, close: async () => { await browser.keys(['Escape']); } },
];

describe('8 Schriften und Symbole zur Laufzeit', () => {
  before(async () => { await openCatalog(1280); });
  it('self-check: reports a wrong font and a forbidden glyph', async () => {
    await railTo('Katalog');
    await browser.execute(() => {
      const el = document.createElement('span');
      el.id = 'mfk-selftest';
      el.textContent = '✓ Test';
      el.style.cssText = 'position:fixed;top:120px;left:120px;font-family:serif;z-index:99999';
      document.body.appendChild(el);
    });
    const problems = await runAudit('typography');
    await browser.execute(() => document.getElementById('mfk-selftest')?.remove());
    const checks = problems.map((p) => p.check);
    expect(checks).toContain('font');
    expect(checks).toContain('glyph');
  });

  for (const view of views) {
    it(view.name, async () => {
      await view.open();
      await browser.pause(300);
      const problems = await runAudit('typography');
      await view.close?.();
      await expectNoProblems(problems, `typography ${view.name}`);
    });
  }
});
