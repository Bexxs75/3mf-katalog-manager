import { openCatalog, railTo } from '../lib/app.js';
import { byText, click, sleep, waitForText } from '../lib/ui.js';

describe('4 Printer Manager', () => {
  before(async () => {
    await openCatalog();
    await railTo('Printer Manager');
    await waitForText('h1', 'Testdrucker Alpha');
  });

  const nozzle = () => $('input[aria-label="Düse"]');

  it('keeps the edit form open after a real click on "Bearbeiten" (regression: save-on-open)', async () => {
    expect(await (await nozzle()).getProperty('readOnly')).toBe(true);
    await click((await byText('button', 'Bearbeiten'))!);
    await waitForText('button', 'Speichern');
    await waitForText('button', 'Abbrechen');
    // The old bug saved and closed the form with the very same click; give it time to show up.
    await sleep(800);
    expect(await byText('button', 'Speichern')).not.toBeNull();
    expect(await byText('button', 'Abbrechen')).not.toBeNull();
    expect(await (await nozzle()).getProperty('readOnly')).toBe(false);
  });

  it('rejects a nozzle of 5 mm with the validation message', async () => {
    await click(await nozzle());
    await browser.keys(['Control', 'a']);
    await browser.keys(['5']);
    await click((await byText('button', 'Speichern'))!);
    await waitForText('p', 'Düse muss zwischen 0,1 und 2,0 mm liegen.');
    expect(await byText('button', 'Speichern')).not.toBeNull();
  });

  it('saves a valid nozzle and leaves edit mode', async () => {
    await click(await nozzle());
    await browser.keys(['Control', 'a']);
    await browser.keys('0,4'.split(''));
    await click((await byText('button', 'Speichern'))!);
    await waitForText('button', 'Bearbeiten');
    await browser.waitUntil(async () => (await (await nozzle()).getValue()) === '0,4');
    expect(await (await nozzle()).getProperty('readOnly')).toBe(true);
  });
});
