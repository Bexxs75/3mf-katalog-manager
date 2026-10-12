import { closeSettings, openCatalog, openSettings } from '../lib/app.js';
import { button } from '../lib/catalog-ui.js';
import { click } from '../lib/ui.js';

describe('15 Fehler melden ohne Versand', () => {
  const dialog = () => $('[role="dialog"][aria-labelledby="bug-report-title"]');
  before(async () => { await openCatalog(); await openSettings('Info'); });
  after(async () => { await closeSettings(); });

  it('shows the required log choice and closes with Abbrechen', async () => {
    await button('Fehler melden');
    await dialog().waitForDisplayed();
    expect(await dialog().$('h2').getText()).toContain('Fehler melden');
    const labels = await dialog().$$('label');
    expect(await labels.length).toBe(2);
    expect(await labels[0].getText()).toBe('Ja, Logdatei anhängen');
    expect(await labels[1].getText()).toBe('Nein, ohne Logdatei melden');
    for (const label of labels) {
      expect(await label.isDisplayed()).toBe(true);
      expect(await label.$('input[type="radio"]').isSelected()).toBe(false);
    }
    // Required report fields live in an external browser form, beyond the offline test boundary.
    const open = dialog().$('button=Formular öffnen');
    if (await open.isExisting()) expect(await open.isEnabled()).toBe(false);
    expect(await dialog().$('a').isExisting()).toBe(false);
    await click(dialog().$('button=Abbrechen'));
    await dialog().waitForExist({ reverse: true });
  });

  it('reopens and closes with Escape without opening or submitting the external form', async () => {
    await button('Fehler melden');
    await dialog().waitForDisplayed();
    await browser.keys(['Escape']);
    await dialog().waitForExist({ reverse: true });
    expect(await $('button=Fehler melden').isDisplayed()).toBe(true);
  });
});
