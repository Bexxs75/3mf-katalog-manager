import { cardByName } from './app.js';
import { click, hover, typeKeys, waitForText, type El } from './ui.js';

export const detailPanel = () => $('[role="complementary"][aria-label="Details"]');

export async function button(text: string) {
  await click(await waitForText('button', text));
}

export async function closeDetails() {
  const close = await $('button[aria-label="Details schließen"]');
  if (await close.isExisting()) await click(close);
}

export async function catalogCount(count: number) {
  await browser.waitUntil(async () => (await $('header').getText()).includes(`${count} Dateien`), {
    timeoutMsg: `expected ${count} Dateien in the catalog header`,
  });
}

export async function selectCards(names: string[]) {
  await closeDetails();
  for (const name of names) {
    const card = await cardByName(name);
    await hover(card);
    await click(card.$('label'));
    expect(await card.$('input[type="checkbox"]').isSelected()).toBe(true);
  }
  await waitForText('span', `${names.length} ausgewählt`);
}

/** Native selects need keyboard input: selecting options through script bypasses real input. */
export async function chooseOption(select: El, label: string) {
  const el = (await select) as WebdriverIO.Element;
  const options = await browser.execute((node: HTMLSelectElement) => Array.from(node.options).map(o => ({ text: o.text, value: o.value })), el as unknown as HTMLSelectElement);
  const index = options.findIndex(o => o.text === label);
  if (index < 0) throw new Error(`Missing option ${label}`);
  await click(el);
  await browser.keys(['Home']);
  for (let i = 0; i < index; i++) await browser.keys(['ArrowDown']);
  await browser.keys(['Enter']);
  await browser.waitUntil(async () => await el.getValue() === options[index].value);
}

/** Explicit Shift preserves collection-name casing on WebKitWebDriver. */
export async function typeCaseSensitive(text: string) {
  for (const char of text) {
    if (/[A-Z]/.test(char)) {
      await browser.keys(['Shift', char.toLowerCase()]);
    } else await typeKeys(char);
  }
}
