// Navigation and lookup helpers shared by the specs.
import { spawnSync } from 'node:child_process';
import { click, reveal, sleep, xpathLiteral } from './ui.js';

export const T = {
  appHeader: '3MF KATALOG',
  filesCount: (n: number) => `${n} Dateien`,
  helpLabel: 'Tastenkürzel und Tipps',
};

export async function waitForWindow() {
  // The window is shown only after the frontend reported ready; until then the
  // viewport is 0x0 and every geometry check would be meaningless.
  await browser.waitUntil(async () => (await browser.execute(() => window.innerWidth)) > 0, { timeout: 30000, timeoutMsg: 'window never got a size' });
}

export async function setViewport(width: number, height = 800) {
  await browser.setWindowRect(null, null, width, height);
  await browser.waitUntil(async () => (await browser.execute(() => window.innerWidth)) === width, { timeout: 10000, timeoutMsg: `viewport did not become ${width} px wide` });
}

/** Finished first-run state, so tests do not have to go through the setup dialog. */
export async function configureFrontend(catalogDir: string, pinnedDetails = false) {
  // Right after the window appears the document can still be a blank page whose localStorage is
  // "access denied" (seen on Windows runners); repeat until the app's page answers.
  await browser.waitUntil(async () => {
    try {
      await browser.execute((dir: string, pinned: boolean) => {
        localStorage.setItem('3mf-katalog-base-dir', dir);
        localStorage.setItem('3mf-katalog-setup-seen', '1');
        localStorage.setItem('3mf-katalog-language', 'de');
        localStorage.setItem('3mf-katalog-theme', 'light');
        localStorage.setItem('3mf-katalog-dnd-tip-dismissed', 'true');
        // The automatic detail panel appears only after a successful selection; a pinned one is always there.
        if (pinned) localStorage.setItem('3mf-katalog-detail-panel', 'pinned');
      }, catalogDir, pinnedDetails);
      return true;
    } catch { return false; }
  }, { timeout: 60000, interval: 500, timeoutMsg: 'the frontend did not allow access to localStorage within 60 s' });
  await browser.refresh();
  await waitForWindow();
}

/** Starts from a configured, loaded catalog at the given width. */
export async function openCatalog(width = 1280, height = 800, options: { pinnedDetails?: boolean } = {}) {
  await waitForWindow();
  await configureFrontend(process.env.MFK_E2E_CATALOG!, options.pinnedDetails);
  await setViewport(width, height);
  await $('[data-model-id]').waitForExist({ timeout: 20000 });
}

export async function railTo(label: 'Katalog' | 'Material Manager' | 'Printer Manager' | 'Papierkorb') {
  await click($(`[aria-label="${label}"]`));
}

/** Clicks a rail entry and waits for its view; a click swallowed while the app settles is repeated (a repeat on the active entry is harmless). */
export async function railToAndWait(label: Parameters<typeof railTo>[0], ready: () => Promise<boolean>) {
  for (let attempt = 0; attempt < 3; attempt++) {
    await railTo(label);
    try {
      await browser.waitUntil(async () => { try { return await ready(); } catch { return false; } }, { timeout: 8000, interval: 250 });
      return;
    } catch { /* view did not appear, click again */ }
  }
  throw new Error(`the view "${label}" did not appear`);
}

async function settingsTab(name: string) {
  const el = await $(`//button[@role="tab"][normalize-space()=${xpathLiteral(name)}]`);
  return (await el.isExisting()) ? el : null;
}

async function settingsTabAppears(name: string, timeout = 10000) {
  const el = await $(`//button[@role="tab"][normalize-space()=${xpathLiteral(name)}]`);
  await el.waitForExist({ timeout, timeoutMsg: `no settings tab "${name}"` });
  return el;
}

export async function openSettings(tab?: 'Allgemein' | 'Slicer' | 'Katalog' | 'Info') {
  // A click right after startup is sometimes swallowed while the app still settles (slow CI runners):
  // open again when the view did not appear, never click while it is already open (that would close it).
  const probe = tab ?? 'Allgemein';
  for (let attempt = 0; attempt < 3; attempt++) {
    if (await settingsTab(probe)) break;
    // The panel itself is also labelled "Einstellungen"; only the rail button toggles it.
    await click(await $('button[aria-label="Einstellungen"]'));
    try { await settingsTabAppears(probe, 6000); break; } catch { /* try the click again */ }
  }
  if (tab) await click(await settingsTabAppears(tab));
}

export async function closeSettings() {
  const close = await $('[aria-label="Einstellungen schließen"]');
  if (await close.isExisting()) await click(close);
}

export async function cardByName(name: string) {
  const card = await $(`//div[@data-model-id][.//div[@data-card-metadata]/div[normalize-space()=${xpathLiteral(name)}]]`);
  await card.waitForExist({ timeout: 10000, timeoutMsg: `no card named ${name}` });
  return card;
}

/** Sidebar row (folder, collection) by its visible name. */
export async function sidebarRow(name: string) {
  const lit = xpathLiteral(name);
  const row = await $(`//aside//div[@role="button"][.//span[normalize-space()=${lit}]] | //aside//*[self::div or self::button][normalize-space(text())=${lit}]`);
  await row.waitForExist({ timeout: 10000, timeoutMsg: `no sidebar entry ${name}` });
  return row;
}

export async function expandCatalogRoot() {
  const root = await sidebarRow('Katalog');
  const chevron = await root.$('span');
  await reveal(chevron);
  await click(chevron);
  await sidebarRow('Deko');
}

/** Titles of the native top-level windows (Linux only; WebDriver cannot read them). */
export function nativeWindowTitles(): string[] {
  const out = spawnSync('xwininfo', ['-root', '-tree'], { encoding: 'utf8' }).stdout ?? '';
  return out.split('\n').map((l) => /"([^"]+)"/.exec(l)?.[1]).filter((t): t is string => !!t);
}

export { sleep };

/** A timeout remains a failure; only the app's explicit WebGL fallback allows a skip. */
export async function waitForPlateViewer(): Promise<boolean> {
  const bar = () => $('[data-plate-selector]');
  const unsupported = () => $('//*[@data-detail-viewer]//h3[normalize-space()="Die 3D-Ansicht wird auf diesem Rechner nicht unterstützt"]');
  await browser.waitUntil(async () => await bar().isDisplayed() || await unsupported().isDisplayed(), {
    timeout: 20000, timeoutMsg: 'Neither plate controls nor an explicit WebGL-unavailable message appeared',
  });
  return await bar().isDisplayed();
}
