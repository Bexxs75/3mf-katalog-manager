import { openCatalog } from '../lib/app.js';
import { typeCaseSensitive } from '../lib/catalog-ui.js';
import { click, waitForText } from '../lib/ui.js';

describe('12 Sammlungen sortieren', () => {
  before(async () => { await openCatalog(); });

  it('inserts mixed-case names and numbers in natural order in sidebar and gallery', async () => {
    const created = ['Zebra', 'apfel', 'Kiste 10', 'Kiste 2'];
    for (const name of created) {
      // The insertion point moves after each collection; always locate it again.
      await click(await waitForText('div', '+ Neue Sammlung'));
      const input = $('input[placeholder="Name der Sammlung"]');
      await input.waitForDisplayed();
      await click(input);
      await typeCaseSensitive(name);
      expect(await input.getValue()).toBe(name);
      await browser.keys(['Enter']);
      await $(`aside [role="button"][aria-label="${name}"]`).waitForDisplayed();
    }
    const expected = ['apfel', 'Kiste 2', 'Kiste 10', 'Testsammlung', 'Zebra'];
    const sidebar = await browser.execute((names: string[]) => Array.from(document.querySelectorAll('aside [role="button"][aria-label]'))
      .map(el => el.getAttribute('aria-label')).filter(name => name && names.includes(name)), expected);
    expect(sidebar).toEqual(expected);
    await click(await waitForText('span', 'alle anzeigen'));
    await waitForText('span', 'Zurück zum Katalog');
    const gallery = await browser.execute((names: string[]) => Array.from(document.querySelectorAll('[data-catalog-main] span'))
      .map(el => el.textContent?.trim()).filter(name => name && names.includes(name)), expected);
    expect(gallery).toEqual(expected);
  });
});
