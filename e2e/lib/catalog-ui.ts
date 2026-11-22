import { cardByName } from './app.js';
import { click, hover, typeKeys, waitForText, type El } from './ui.js';

export const detailPanel = () => $('[role="complementary"][aria-label="Details"]');

export async function button(text: string) {
  await click(await waitForText('button', text));
}

/** Bulk actions live in the selection bar's grouped menus; the group is named by its button label. */
export async function moreAction(text: string, group: string) {
  await click(await $(`//*[@role="toolbar"]//button[@aria-haspopup="menu"][normalize-space()="${group}"]`));
  await click(await $(`//*[@role="menu"]//button[@role="menuitem"][normalize-space()="${text}"]`));
}

/**
 * An entry of the model context menu. Entries carry a shortcut hint in their
 * visible text, so they are matched by their accessible name; the selection
 * bar and the details panel have look-alike buttons of their own.
 */
export const contextEntry = (label: string) =>
  $(`//button[(@aria-label="${label}" or normalize-space()="${label}") and not(ancestor::*[@role="toolbar"]) and not(ancestor::aside)]`);

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
  // A pinned detail panel stays; the selection bar is the signal that the layout has settled.
  await $('[role="toolbar"]').waitForDisplayed();
  for (const name of names) {
    const card = await cardByName(name);
    await hover(card);
    await click(card.$('label'));
    expect(await card.$('input[type="checkbox"]').isSelected()).toBe(true);
  }
  await waitForText('span', `${names.length} ausgewählt`);
}

/**
 * The custom Select renders a combobox button plus a portalled listbox. Open it
 * with a real click and pick the option with a real click, then require the
 * trigger to show the chosen text so a swallowed click cannot pass unnoticed.
 */
export async function chooseOption(select: El, label: string) {
  const trigger = (await select) as WebdriverIO.Element;
  await click(trigger);
  const listId = await browser.waitUntil(async () => await trigger.getAttribute('aria-controls'), { timeoutMsg: 'select popup did not open' });
  const option = await $(`//*[@id="${listId}"]//*[@role="option"][normalize-space()="${label}"]`);
  if (!await option.isExisting()) throw new Error(`Missing option ${label}`);
  await click(option);
  await browser.waitUntil(async () => (await trigger.getText()).trim() === label, { timeoutMsg: `select trigger did not show ${label}` });
}

/** Explicit Shift preserves collection-name casing on WebKitWebDriver. */
export async function typeCaseSensitive(text: string) {
  for (const char of text) {
    if (/[A-Z]/.test(char)) {
      await browser.keys(['Shift', char.toLowerCase()]);
    } else await typeKeys(char);
  }
}

/** Selects a model and waits until the detail panel shows it; a swallowed click is repeated. */
export async function selectModel(name: string) {
  for (let attempt = 0; attempt < 3; attempt++) {
    await click(await cardByName(name));
    try {
      await browser.waitUntil(async () => (await detailPanel().isDisplayed()) && (await detailPanel().getText()).includes(name), { timeout: 5000 });
      return;
    } catch { /* click did not select; the next click tries again */ }
  }
  throw new Error(`the detail panel did not show ${name}`);
}
