// Navigation and lookup helpers shared by the specs.
import { spawnSync } from 'node:child_process';
import { click, byText, reveal, sleep, xpathLiteral } from './ui.js';

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
export async function configureFrontend(catalogDir: string) {
  await browser.execute((dir: string) => {
    localStorage.setItem('3mf-katalog-base-dir', dir);
    localStorage.setItem('3mf-katalog-setup-seen', '1');
    localStorage.setItem('3mf-katalog-language', 'de');
    localStorage.setItem('3mf-katalog-theme', 'light');
    localStorage.setItem('3mf-katalog-dnd-tip-dismissed', 'true');
  }, catalogDir);
  await browser.refresh();
  await waitForWindow();
}

/** Starts from a configured, loaded catalog at the given width. */
export async function openCatalog(width = 1280, height = 800) {
  await waitForWindow();
  await configureFrontend(process.env.MFK_E2E_CATALOG!);
  await setViewport(width, height);
  await $('[data-model-id]').waitForExist({ timeout: 20000 });
}

export async function railTo(label: 'Katalog' | 'Material Manager' | 'Printer Manager' | 'Papierkorb') {
  await click($(`[aria-label="${label}"]`));
}

export async function openSettings(tab?: 'Allgemein' | 'Slicer' | 'Katalog' | 'Info') {
  const open = await $('[aria-label="Einstellungen"]');
  await click(open);
  if (tab) {
    const tabButton = await byText('button', tab);
    if (!tabButton) throw new Error(`settings tab ${tab} not found`);
    await click(tabButton);
  }
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
