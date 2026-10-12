import { cardByName, openCatalog } from '../lib/app.js';
import { button, closeDetails, detailPanel, selectCards } from '../lib/catalog-ui.js';
import { click, typeKeys } from '../lib/ui.js';

describe('13 Tags für Mehrfachauswahl', () => {
  const names = ['Würfel 20mm.stl', 'Zylinder.stl'];
  const input = () => $('[data-navigation-menu] input[role="combobox"]');
  const tags = async (name: string) => String(await (await cardByName(name)).$('[data-card-tags]').getProperty('textContent'));
  before(async () => { await openCatalog(); });

  it('suggests an existing tag, accepts it with Down/Enter and cancels an unsubmitted tag', async () => {
    // The seed has no tags; create an existing tag through the same UI a user uses.
    await click(await cardByName('Kegel.stl'));
    const single = detailPanel().$('input[aria-label="Tag hinzufügen"]');
    await single.waitForDisplayed();
    await click(single);
    await typeKeys('werkzeug');
    await browser.keys(['Enter']);
    await browser.waitUntil(async () => (await tags('Kegel.stl'))?.includes('#werkzeug') === true);
    await closeDetails();
    await selectCards(names);
    await button('Tag hinzufügen');
    await input().waitForDisplayed();
    await typeKeys('werk');
    const list = $('[data-navigation-menu] [role="listbox"][aria-label="Tag-Vorschläge"]');
    await list.waitForDisplayed();
    expect(await list.$('[role="option"]').getText()).toBe('werkzeug');
    await browser.keys(['ArrowDown']);
    expect(await list.$('[role="option"]').getAttribute('aria-selected')).toBe('true');
    await browser.keys(['Enter']);
    await input().waitForExist({ reverse: true });
    for (const name of names) await browser.waitUntil(async () => (await tags(name))?.includes('#werkzeug') === true);

    const before = await Promise.all(names.map(tags));
    await button('Tag hinzufügen');
    await input().waitForDisplayed();
    await typeKeys('verworfen');
    await browser.keys(['Escape']);
    await input().waitForExist({ reverse: true });
    expect(await Promise.all(names.map(tags))).toEqual(before);
    await button('Auswahl aufheben');
    for (const name of names) {
      await click(await cardByName(name));
      expect(await detailPanel().getText()).toContain('werkzeug');
      expect(await detailPanel().getText()).not.toContain('verworfen');
      await closeDetails();
    }
  });
});
