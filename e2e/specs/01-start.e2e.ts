import { IS_WINDOWS } from '../lib/run-env.js';
import { nativeWindowTitles, waitForWindow } from '../lib/app.js';

describe('1 Start', () => {
  before(async () => { await waitForWindow(); });

  it('shows the header and no error dialog on a first launch', async () => {
    const header = await $('header');
    await header.waitForDisplayed({ timeout: 20000 });
    expect(await header.getText()).toContain('3MF KATALOG');
    // The error boundary's title; it must not appear on a clean start.
    const body = await $('body').getText();
    expect(body).not.toContain('Etwas ist schiefgelaufen');
    expect(await $('[role="alertdialog"]').isExisting()).toBe(false);
  });

  it('opens a window with a usable size', async () => {
    const size = await browser.execute(() => ({ w: window.innerWidth, h: window.innerHeight }));
    expect(size.w).toBeGreaterThanOrEqual(900);
    expect(size.h).toBeGreaterThanOrEqual(500);
  });

  // The native title is set by the Rust side (product name + version) and is
  // not visible to WebDriver; on Linux the X server can be asked directly.
  (IS_WINDOWS ? it.skip : it)('titles the native window "3MF Katalog Manager [Preview] <version>" (Linux only)', async () => {
    await browser.waitUntil(() => nativeWindowTitles().some((t) => /^3MF Katalog Manager( Preview)? \d+\.\d+\.\d+/.test(t)), {
      timeout: 10000,
      timeoutMsg: `no native window with the expected title, found: ${JSON.stringify(nativeWindowTitles())}`,
    });
  });
});
