// Browser test of W2 "Bereinigung und Abgleich im Web" against the real Docker web image (Forgejo #39, spec 7.5).
// Orchestrated by w2-run.sh (starts the container, builds the throw-away folders with w2-fixture.sh, calls this
// script once per phase inside the Playwright image). It can also be run by hand:
//   node w2-browser-test.mjs PHASE BASE_URL PASSWORD OUT_DIR
// PHASE: seed | ui | startup | emptyroot. Results go to OUT_DIR/results-PHASE.json and stdout (last line: "PASS n/m").
// Needs Playwright and @axe-core/playwright (NODE_PATH, as in the `pw` wrapper). Reads no secrets: the password is
// an argument of this throw-away container only. Screenshots: OUT_DIR/<phase>-<name>-<light|dark>-<1280|400>.png
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const AxeBuilder = require('@axe-core/playwright').default ?? require('@axe-core/playwright');

const [phase = 'ui', base = 'http://127.0.0.1:18090', password = '', out = '/tmp/w2-shots'] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
const results = [];
const findings = [];
let pass = 0;

/** One checked statement; a failed check never stops the run. */
function check(name, ok, detail = '') {
  results.push({ name, ok: !!ok, detail: String(detail) });
  if (ok) pass++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}${detail ? ' :: ' + detail : ''}`);
  return !!ok;
}
const note = (text) => { findings.push(text); console.log('NOTE ' + text); };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));

// ---- API (no browser): used to seed the catalog and for counters ----------------------------------------------
class Api {
  cookie = ''; csrf = '';
  async login() {
    const res = await fetch(base + '/api/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password }) });
    if (res.status !== 200) throw new Error('login ' + res.status);
    this.cookie = (res.headers.getSetCookie?.() ?? []).map(c => c.split(';')[0]).join('; ');
    this.csrf = (await res.json()).csrfToken;
    return this;
  }
  async invoke(cmd, args = {}) {
    const res = await fetch(base + '/api/invoke/' + cmd, {
      method: 'POST', headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': this.csrf, Cookie: this.cookie, Origin: base, 'X-Request-Id': crypto.randomUUID() },
      body: JSON.stringify(args),
    });
    const text = await res.text();
    let json; try { json = JSON.parse(text); } catch { json = text; }
    if (!res.ok) { const e = new Error(`${cmd} -> ${res.status} ${text.slice(0, 200)}`); e.status = res.status; e.body = json; throw e; }
    return json;
  }
}

/** Takes the whole report over through the same commands as the dialog (used to build the base state). */
async function applyAllNew(api) {
  const report = await api.invoke('disk_sync_report');
  const preview = await api.invoke('disk_sync_preview', { newFiles: report.newFiles, missingModels: [], danglingCompanions: [] });
  if (!preview.token) return { report, imported: 0 };
  await api.invoke('disk_sync_apply_start', { token: preview.token, confirmRemoval: false });
  let imported = 0;
  for (let seq = 0; ; seq++) {
    const block = await api.invoke('disk_sync_apply_block', { token: preview.token, seq });
    imported = block.counts.imported;
    if (block.remaining === 0) break;
  }
  return { report, imported };
}

// ---- browser helpers --------------------------------------------------------------------------------------------
const VIEWS = [
  { scheme: 'light', width: 1280, height: 900 },
  { scheme: 'dark', width: 1280, height: 900 },
  { scheme: 'light', width: 400, height: 800 },
  { scheme: 'dark', width: 400, height: 800 },
];
async function newPage(browser, view = VIEWS[0]) {
  const context = await browser.newContext({ viewport: { width: view.width, height: view.height }, colorScheme: view.scheme, locale: 'de-DE' });
  const page = await context.newPage();
  page.on('pageerror', e => console.log('PAGEERROR ' + e.message));
  return page;
}
async function login(page) {
  await page.goto(base + '/', { waitUntil: 'domcontentloaded' });
  const input = page.locator('input[type="password"]').first();
  await input.waitFor({ timeout: 20000 });
  await input.fill(password);
  await input.press('Enter');
  await page.getByRole('button', { name: 'Einstellungen' }).first().waitFor({ timeout: 30000 });
}
async function shot(page, name, view) {
  const file = `${out}/${phase}-${name}-${view.scheme}-${view.width}.png`;
  await page.screenshot({ path: file });
  return file;
}
/** Text of everything that has focus, for tab order checks. */
const focusInfo = (page) => page.evaluate(() => {
  const el = document.activeElement;
  if (!el || el === document.body) return { name: '(body)', inDialog: false };
  const label = el.getAttribute('aria-label') || el.innerText?.trim().slice(0, 60) || el.id || el.tagName;
  const cs = getComputedStyle(el);
  const ring = (cs.outlineStyle !== 'none' && parseFloat(cs.outlineWidth) > 0) || cs.boxShadow !== 'none';
  return { name: `${el.tagName.toLowerCase()}:${label}`.replace(/\s+/g, ' '), inDialog: !!el.closest('[role="dialog"]'), ring, tag: el.tagName };
});
const noHScroll = (page) => page.evaluate(() => ({ doc: document.documentElement.scrollWidth - window.innerWidth, body: document.body.scrollWidth - window.innerWidth }));
async function dialogFits(page) {
  return page.evaluate(() => {
    const d = document.querySelector('[role="dialog"]');
    if (!d) return null;
    const r = d.getBoundingClientRect();
    return { left: Math.round(r.left), right: Math.round(r.right), vw: window.innerWidth, scrollW: d.scrollWidth, clientW: d.clientWidth };
  });
}
async function axe(page, label) {
  const res = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
  const heavy = res.violations.filter(v => v.impact === 'serious' || v.impact === 'critical');
  const all = res.violations.map(v => `${v.id}(${v.impact}) x${v.nodes.length}`);
  check(`axe ${label}: keine serious/critical`, heavy.length === 0, heavy.map(v => `${v.id}:${v.nodes.map(n => n.target.join(' ')).slice(0, 2).join('|')}`).join('; ') || (all.length ? 'nur: ' + all.join(', ') : 'keine'));
  return res.violations;
}

// PHASES
const phaseFns = {};

phaseFns.seed = async () => {
  const api = await new Api().login();
  const { report, imported } = await applyAllNew(api);
  check('seed: Bericht nennt 36 neue Dateien (30 alpha + 6 beta)', report.newFiles.length === 36, `newFiles=${report.newFiles.length}`);
  check('seed: alle 36 aufgenommen', imported === 36, `imported=${imported}`);
  const again = await api.invoke('disk_sync_report');
  check('seed: zweiter Bericht ohne Unterschiede', again.newFiles.length + again.missingModels.length + again.danglingCompanions.length === 0,
    `neu=${again.newFiles.length} fehlt=${again.missingModels.length} unklar=${(again.unclear ?? []).length}`);
  check('seed: zwei Wurzeln mit Status ok', again.roots.length === 2 && again.roots.every(r => r.status === 'ok'), JSON.stringify(again.roots));
};


// ---- phase helpers ------------------------------------------------------------------------------------------------
const FIRST = VIEWS[0];
/** Screenshots in all four views (light/dark x 1280/400) of the page as it is now, plus the layout checks; ends in light 1280. */
async function shotAll(page, name, { withAxe = true } = {}) {
  for (const view of VIEWS) {
    await page.setViewportSize({ width: view.width, height: view.height });
    await page.emulateMedia({ colorScheme: view.scheme });
    await sleep(250);
    await shot(page, name, view);
    if (view.width === 400) {
      const sc = await noHScroll(page);
      check(`${name} ${view.scheme} 400px: kein horizontaler Seiten-Scroll`, sc.doc <= 0 && sc.body <= 0, `doc=${sc.doc} body=${sc.body}`);
      const fit = await dialogFits(page);
      if (fit) check(`${name} ${view.scheme} 400px: Dialog innerhalb des Fensters, ohne eigenen Quer-Scroll`, fit.left >= 0 && fit.right <= fit.vw && fit.scrollW <= fit.clientW + 1, JSON.stringify(fit));
    } else if (withAxe && await page.locator('[role="dialog"]').count()) {
      await axe(page, `${name} ${view.scheme} 1280`);
    }
  }
  await page.setViewportSize({ width: FIRST.width, height: FIRST.height });
  await page.emulateMedia({ colorScheme: FIRST.scheme });
}
/** Presses Tab up to `max` times; returns what got focus. Stops when the first element comes round again. */
async function tabOrder(page, max = 40) {
  const seen = [];
  for (let i = 0; i < max; i++) {
    await page.keyboard.press('Tab');
    const info = await focusInfo(page);
    if (seen.length && info.name === seen[0].name) break;
    seen.push(info);
  }
  return seen;
}
/** axe on the open dialog in all four views (shotAll only runs it at 1280 px). */
async function axeAllViews(page, name) {
  for (const view of VIEWS) {
    await page.setViewportSize({ width: view.width, height: view.height });
    await page.emulateMedia({ colorScheme: view.scheme });
    await sleep(250);
    await axe(page, `${name} ${view.scheme} ${view.width}`);
  }
  await page.setViewportSize({ width: FIRST.width, height: FIRST.height });
  await page.emulateMedia({ colorScheme: FIRST.scheme });
}
const dialog = (page) => page.locator('[role="dialog"]').first();
const heading = (page) => dialog(page).locator('h2').first();
async function waitStep(page, step, timeout = 30000) { await page.locator(`[role="dialog"][data-step="${step}"]`).waitFor({ timeout }); }
async function openAbgleich(page) {
  const trigger = page.getByRole('button', { name: 'Mit Festplatte abgleichen' });
  await trigger.click();
  await waitStep(page, 'select');
  return trigger;
}
const isFocused = (locator) => locator.evaluate(el => el === document.activeElement);

// ---- phase: ui ------------------------------------------------------------------------------------------------------
phaseFns.ui = async () => {
  const browser = await chromium.launch();
  const page = await newPage(browser);
  await login(page);
  await page.waitForTimeout(1500);
  const api = await new Api().login();

  // ===== A. Bereinigungsdialog (Seitenleiste "Aufräum-Vorschläge", Einstellungen > Wartung) =====
  const sidebarTrigger = page.getByRole('button', { name: 'Aufräum-Vorschläge' });
  await sidebarTrigger.click();
  await dialog(page).waitFor();
  await page.getByText('Ordner fehlt, 4 Modelle', { exact: false }).first().waitFor();
  const cleanup = dialog(page);
  check('Bereinigung: Dialog heißt "Aufräum-Vorschläge"', await cleanup.getAttribute('aria-label') === 'Aufräum-Vorschläge');
  check('Bereinigung: Fokus beim Öffnen im Dialog (Abbrechen)', (await focusInfo(page)).inDialog && (await focusInfo(page)).name.includes('Abbrechen'), (await focusInfo(page)).name);
  check('Bereinigung: zwei Ordnergruppen "Ordner fehlt, 3 / 4 Modelle" mit je einem Knopf', await cleanup.getByRole('button', { name: /^\d Modelle in den Papierkorb$/ }).count() === 2);
  const orphanBoxes = cleanup.locator('label:has(input[type=checkbox])');
  const checkedBoxes = await cleanup.locator('input[type=checkbox]:checked').count();
  check('Bereinigung: 4 verwaiste Einzelmodelle + 2 Duplikat-Kopien vorausgewählt, Ordnergruppen nicht', checkedBoxes === 6, `angehakt=${checkedBoxes} von ${await orphanBoxes.count()}`);
  const unclear = cleanup.locator('[data-testid="disk-unclear"]');
  check('Bereinigung: "Unklar" ist eingeklappt', await unclear.count() === 1 && !(await unclear.evaluate(el => el.open)));
  check('Bereinigung: "Unklar" zeigt 2 Einträge, nur Anzeige (keine Kontrollkästchen)', (await unclear.innerText()).includes('2 Einträge') && await unclear.locator('input').count() === 0);
  await unclear.locator('summary').click();
  const unclearText = await unclear.innerText();
  note(`Bereinigung, Unklar aufgeklappt zeigt: ${unclearText.replace(/\s+/g, ' ').trim()}`);
  // scan_catalog_issues sends `roots` (display names by index), so with two roots the rows read "alpha: Sub/..." like in the reconciliation dialog.
  check('Bereinigung: Unklar-Zeilen nennen Wurzelname und Pfad ("alpha: Sub/Einzelteil.3mf")', /alpha: Sub\/Einzelteil\.3mf/.test(unclearText) && /alpha: Verlinkt\/Linkteil\.3mf/.test(unclearText));
  check('Bereinigung: Unklar-Zeilen haben einen Grund', /link|Link|nicht erreichbar|gelesen|geprüft werden/i.test(unclearText), unclearText.replace(/\s+/g, ' ').slice(0, 200));
  await shotAll(page, 'cleanup-open');
  // keyboard: tab order, focus ring, trap
  const tabs = await tabOrder(page, 30);
  note('Bereinigung Tab-Reihenfolge: ' + tabs.map(t => t.name).join(' > '));
  check('Bereinigung: Tab bleibt im Dialog (Fokusfalle)', tabs.length > 3 && tabs.every(t => t.inDialog));
  check('Bereinigung: jedes fokussierte Element zeigt einen Fokus-Ring', tabs.every(t => t.ring), tabs.filter(t => !t.ring).map(t => t.name).join('; '));
  const closeX = cleanup.locator('span:has(> svg)').first();
  check('Bereinigung: Schließen-Kreuz oben rechts ist per Tab erreichbar', tabs.some(t => /schlie|close|×|x/i.test(t.name) && t.tag === 'BUTTON'), 'Kreuz ist ein <span onClick>, kein Button (Befund)');
  // checkboxes by keyboard
  const first = cleanup.locator('input[type=checkbox]').first();
  await first.focus();
  const before = await first.isChecked();
  await page.keyboard.press('Space');
  check('Bereinigung: Leertaste schaltet ein Kontrollkästchen um', await first.isChecked() === !before);
  await page.keyboard.press('Space');
  check('Bereinigung: Leertaste schaltet zurück', await first.isChecked() === before);
  await page.keyboard.press('Escape');
  await cleanup.waitFor({ state: 'detached', timeout: 3000 }).catch(() => {});
  check('Bereinigung: Escape schließt den Dialog', await page.locator('[role="dialog"]').count() === 0);
  const back = await focusInfo(page);
  check('Bereinigung: Fokus kehrt nach Escape zum Auslöser zurück', back.name.includes('Aufräum-Vorschläge'), back.name);
  // via Einstellungen > Wartung
  await page.getByRole('button', { name: 'Einstellungen' }).first().click();
  await page.getByRole('tab', { name: 'Wartung' }).click();
  await page.getByRole('button', { name: /Fehlende Dateien und Duplikate prüfen/ }).click();
  await dialog(page).waitFor();
  check('Bereinigung: Aufruf auch über Einstellungen > Wartung', await dialog(page).getAttribute('aria-label') === 'Aufräum-Vorschläge');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Einstellungen schließen' }).click().catch(() => {});
  // one-click folder removal
  await sidebarTrigger.click();
  await page.getByText('Ordner fehlt, 4 Modelle').first().waitFor();
  await page.getByRole('button', { name: '4 Modelle in den Papierkorb' }).click();
  await page.getByText('4 Modelle in den Papierkorb verschoben').waitFor({ timeout: 20000 }).catch(() => {});
  check('Bereinigung: "4 Modelle in den Papierkorb" entfernt die Ordnergruppe mit einem Klick (Katalog zählt 32 statt 36)', await page.getByText('32 Dateien').count() > 0);
  const stay = dialog(page);
  check('Bereinigung: Dialog bleibt nach dem Entfernen offen und bestätigt "4 Modelle in den Papierkorb verschoben"', await page.locator('[role="dialog"]').count() === 1 && /4 Modelle in den Papierkorb verschoben/.test(await stay.innerText()));
  check('Bereinigung: die entfernte Ordnergruppe ist weg, die andere Gruppe und die Einzelmodelle bleiben', await stay.getByText('Ordner fehlt, 4 Modelle').count() === 0 && await stay.getByText(/Ordner fehlt, \d Modelle/).count() === 1 && await stay.locator('input[type=checkbox]').count() > 0);
  const afterFocus = await focusInfo(page);
  check('Bereinigung: Fokus bleibt nach dem Entfernen im Dialog (nicht auf body)', afterFocus.inDialog, afterFocus.name);
  await shotAll(page, 'cleanup-after-folder', { withAxe: false });
  await axeAllViews(page, 'cleanup-after-folder (Bestätigung)');
  await page.keyboard.press('Escape');
  await page.locator('[role="dialog"]').waitFor({ state: 'detached', timeout: 5000 }).catch(() => {});

  // ===== B. Abgleich-Dialog =====
  const abgleichTrigger = await openAbgleich(page);
  check('Abgleich: Fokus beim Öffnen auf der Überschrift im Dialog', (await focusInfo(page)).name.startsWith('h2') && (await focusInfo(page)).inDialog, (await focusInfo(page)).name);
  const dlg = dialog(page);
  const text = await dlg.innerText();
  check('Abgleich: Zähl-Chips (31 neu, 4 fehlen im Einzelnen, 2 unklar)', /31 neue Dateien/.test(text) && /2 unklar/.test(text), text.split('\n').slice(0, 6).join(' / '));
  check('Abgleich: Ordnergruppe "Ordner fehlt" (Archiv-alt) sichtbar, nicht vorausgewählt', /Archiv-alt/.test(text) && await dlg.locator('section[aria-labelledby="disk-report-folders"] input:checked').count() === 0);
  check('Abgleich: "Neu" vorausgewählt (31), "Fehlt" nicht', await dlg.locator('section[aria-labelledby="disk-report-new"] input:checked').count() === 31 && await dlg.locator('section[aria-labelledby="disk-report-missing"] input:checked').count() === 0);
  check('Abgleich: "Weiter" nennt 31 Änderungen', await dlg.getByRole('button', { name: /^Weiter \(31 Änderungen\)$/ }).count() === 1);
  const ab = dlg.locator('[data-testid="disk-unclear"]');
  check('Abgleich: "Unklar" eingeklappt, nur Anzeige, nie vorausgewählt', await ab.count() === 1 && !(await ab.evaluate(el => el.open)) && await ab.locator('input').count() === 0);
  await ab.locator('summary').click();
  const abUnclear = (await ab.innerText()).replace(/\s+/g, ' ');
  note(`Abgleich, Unklar aufgeklappt zeigt: ${abUnclear}`);
  check('Abgleich: Unklar-Zeilen nennen Wurzelname und Ordner ("alpha: Sub")', /alpha: Sub/.test(abUnclear) && /alpha: Verlinkt/.test(abUnclear));
  await shotAll(page, 'abgleich-select');
  await ab.locator('summary').click();
  const abTabs = await tabOrder(page, 60);
  note('Abgleich Tab-Reihenfolge (select): ' + abTabs.map(t => t.name).join(' > '));
  check('Abgleich: Tab bleibt im Dialog', abTabs.length > 5 && abTabs.every(t => t.inDialog));
  check('Abgleich: jedes fokussierte Element zeigt einen Fokus-Ring', abTabs.every(t => t.ring), abTabs.filter(t => !t.ring).map(t => t.name).join('; '));
  // keyboard on a "Fehlt" checkbox
  const missingBox = dlg.locator('section[aria-labelledby="disk-report-missing"] input[type=checkbox]').first();
  await missingBox.focus();
  await page.keyboard.press('Space');
  check('Abgleich: Leertaste hakt ein "Fehlt"-Kästchen an, "Weiter" zählt 32', await missingBox.isChecked() && await dlg.getByRole('button', { name: /^Weiter \(32 Änderungen\)$/ }).count() === 1);
  await page.keyboard.press('Space');
  check('Abgleich: Leertaste nimmt den Haken zurück (31)', !(await missingBox.isChecked()) && await dlg.getByRole('button', { name: /^Weiter \(31 Änderungen\)$/ }).count() === 1);
  // Alle/Keine of a section
  await dlg.getByRole('button', { name: /Keine.*Neu|Keine/ }).first().click();
  check('Abgleich: "Keine" im Abschnitt Neu leert die Auswahl, "Weiter" gesperrt', await dlg.getByRole('button', { name: /^Weiter$/ }).isDisabled());
  await dlg.getByRole('button', { name: /Alle auswählen/ }).first().click();
  // select the folder group (Archiv-alt) as well and go to the preview
  await dlg.locator('section[aria-labelledby="disk-report-folders"] input[type=checkbox]').first().check();
  await dlg.getByRole('button', { name: /^Weiter \(34 Änderungen\)$/ }).click();
  await waitStep(page, 'preview');
  check('Abgleich: Vorschau-Überschrift hat den Fokus', (await focusInfo(page)).name.startsWith('h2') && /Vorschau/.test((await focusInfo(page)).name), (await focusInfo(page)).name);
  const pv = await dialog(page).innerText();
  check('Abgleich: Vorschau nennt 31 aufnehmen, 3 entfernen (Papierkorb), Sicherungshinweis', /31/.test(pv) && /3/.test(pv) && /Sicherungskopie/.test(pv), pv.replace(/\s+/g, ' ').slice(0, 300));
  check('Abgleich: "Änderungen übernehmen" ist die Bestätigung und bedienbar', await dialog(page).getByRole('button', { name: 'Änderungen übernehmen' }).isEnabled());
  await shotAll(page, 'abgleich-preview');
  await dialog(page).getByRole('button', { name: 'Zurück' }).click();
  await waitStep(page, 'select');
  check('Abgleich: "Zurück" behält die Auswahl (34)', await dialog(page).getByRole('button', { name: /^Weiter \(34 Änderungen\)$/ }).count() === 1);
  await dialog(page).getByRole('button', { name: /^Weiter \(34 Änderungen\)$/ }).click();
  await waitStep(page, 'preview');
  // run with slowed blocks: Escape must not close, "Abbrechen" stops after the running block
  await page.route('**/api/invoke/disk_sync_apply_block', async route => { await sleep(3000); await route.continue(); });
  await dialog(page).getByRole('button', { name: 'Änderungen übernehmen' }).click();
  await page.locator('[role="progressbar"]').waitFor({ timeout: 15000 });
  check('Abgleich: Fortschritt als Fortschrittsbalken mit Wert, Fokus auf "Abbrechen"', (await focusInfo(page)).name.includes('Abbrechen'), (await focusInfo(page)).name);
  await shotAll(page, 'abgleich-running');
  await page.keyboard.press('Escape');
  await sleep(300);
  check('Abgleich: Escape während des Laufs schließt den Dialog nicht', await page.locator('[role="dialog"]').count() === 1);
  await dialog(page).getByRole('button', { name: 'Abbrechen' }).click();
  await page.getByText('Wird nach dem laufenden Block gestoppt').waitFor({ timeout: 5000 });
  await waitStep(page, 'cancelled', 60000);
  await page.unroute('**/api/invoke/disk_sync_apply_block');
  const cancelled = (await dialog(page).innerText()).replace(/\s+/g, ' ');
  const m = cancelled.match(/Abgebrochen nach (\d+) von (\d+)/i);
  check('Abgleich: "Abgebrochen nach n von 34", n ein ganzer Block (10) und kleiner als 34', !!m && Number(m[2]) === 34 && Number(m[1]) >= 10 && Number(m[1]) < 34, cancelled.slice(0, 260));
  check('Abgleich: Ergebnis-Überschrift (h3 "Abgebrochen nach …") hat nach dem Abbruch den Fokus', /^h3:abgebrochen nach/i.test((await focusInfo(page)).name), (await focusInfo(page)).name);
  await shotAll(page, 'abgleich-cancelled');
  // rescan, then finish: take over what is left, remove two single missing models (Deko, Technik) and the folder if still there
  await dialog(page).getByRole('button', { name: 'Erneut prüfen' }).click();
  await waitStep(page, 'select');
  const rest = (await dialog(page).innerText()).replace(/\s+/g, ' ');
  note('Abgleich nach Abbruch, erneute Prüfung: ' + rest.slice(0, 160));
  const folderBoxes = dialog(page).locator('section[aria-labelledby="disk-report-folders"] input[type=checkbox]');
  for (let i = 0; i < await folderBoxes.count(); i++) await folderBoxes.nth(i).check();
  for (const name of ['Deko/Modell-01.3mf', 'Technik/Modell-01.3mf']) {
    await dialog(page).locator('label', { hasText: name }).locator('input').check();
  }
  await dialog(page).getByRole('button', { name: /^Weiter \(/ }).click();
  await waitStep(page, 'preview');
  await dialog(page).getByRole('button', { name: 'Änderungen übernehmen' }).click();
  await waitStep(page, 'done', 60000);
  const done = (await dialog(page).innerText()).replace(/\s+/g, ' ');
  note('Abgleich Ergebnis: ' + done.slice(0, 300));
  check('Abgleich: Ergebnisansicht, Ergebnis-Überschrift (h3 "Fertig.") hat den Fokus', /^h3:fertig/i.test((await focusInfo(page)).name), (await focusInfo(page)).name);
  check('Abgleich: Ergebnis nennt aufgenommen/entfernt', /aufgenommen|entfernt/i.test(done), done.slice(0, 200));
  await shotAll(page, 'abgleich-done');
  await dialog(page).getByRole('button', { name: 'Erneut prüfen' }).click();
  await waitStep(page, 'select');
  const after = (await dialog(page).innerText()).replace(/\s+/g, ' ');
  check('Abgleich: danach keine neuen Dateien mehr, noch 2 fehlen (Haushalt/Modell-01, Beta-1), 2 unklar', /0 neue Dateien/.test(after) && /2 Modelle fehlen/.test(after) && /2 unklar/.test(after), after.slice(0, 200));
  await shotAll(page, 'abgleich-after');
  await dialog(page).getByRole('button', { name: 'Später' }).click();
  await page.waitForTimeout(300);
  check('Abgleich: Fokus kehrt nach dem Schließen zu "Mit Festplatte abgleichen" zurück', (await focusInfo(page)).name.includes('Mit Festplatte abgleichen'), (await focusInfo(page)).name);
  // same via Escape
  await openAbgleich(page);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  check('Abgleich: Escape schließt, Fokus beim Auslöser', await page.locator('[role="dialog"]').count() === 0 && (await focusInfo(page)).name.includes('Mit Festplatte abgleichen'), (await focusInfo(page)).name);

  // server-side counters through the API (the shell adds the database counts)
  const counts = await api.invoke('disk_sync_report');
  check('API-Gegenprobe: Bericht nach dem Lauf neu=0 fehlt=2 unklar=2', counts.newFiles.length === 0 && counts.missingModels.length === 2 && (counts.unclear ?? []).length === 2, `neu=${counts.newFiles.length} fehlt=${counts.missingModels.length} unklar=${(counts.unclear ?? []).length}`);

  // ===== C. Datumsumbenennung (3 Modelle: 2 vorhanden, Beta-1 fehlt auf der Platte) =====
  await page.getByPlaceholder(/Name oder Tag suchen/).fill('Beta-');
  await page.waitForTimeout(800);
  const cards = page.locator('input[type=checkbox].sr-only');
  await cards.nth(0).check({ force: true }); await cards.nth(1).check({ force: true }); await cards.nth(2).check({ force: true });
  let opened = false;
  for (const group of ['Markieren', 'Ablegen', 'Bündel']) {
    await page.getByRole('button', { name: group, exact: true }).click();
    const item = page.getByRole('menuitem', { name: /Mit Datum umbenennen/ });
    if (await item.count()) { await item.click(); opened = true; note(`Datumsumbenennung liegt im Menü "${group}"`); break; }
    await page.keyboard.press('Escape');
  }
  check('Datumsumbenennung: Menüeintrag "Mit Datum umbenennen …" im Web vorhanden', opened);
  if (opened) {
    await dialog(page).waitFor();
    await page.getByText(/Vorschau wird geladen/).waitFor({ state: 'detached', timeout: 15000 }).catch(() => {});
    const dr = (await dialog(page).innerText()).replace(/\s+/g, ' ');
    check('Datumsumbenennung: Vorschau zeigt 2 Umbenennungen und 1 übersprungene Datei (fehlt auf der Festplatte)', /Beta-2/.test(dr) && /Datei fehlt auf der Festplatte/.test(dr), dr.slice(0, 300));
    await shotAll(page, 'datum-vorschau');
    await dialog(page).getByRole('button', { name: /^2 Dateien umbenennen$/ }).click();
    await page.getByText(/2 Dateien umbenannt/).first().waitFor({ timeout: 20000 }).catch(() => {});
    check('Datumsumbenennung: Ergebnis-Hinweis "2 Dateien umbenannt"', await page.getByText(/2 Dateien umbenannt/).count() > 0);
    await shotAll(page, 'datum-ergebnis', { withAxe: false });
  }

  // ===== D. Einstellungen: Schalter speichern =====
  await page.getByPlaceholder(/Name oder Tag suchen/).fill('');
  await page.getByRole('button', { name: 'Einstellungen' }).first().click();
  await page.getByRole('tab', { name: 'Katalog' }).click();
  const onStart = page.getByRole('switch', { name: 'Katalogordner nach dem Start abgleichen' });
  const autoAdd = page.getByRole('switch', { name: 'Neue Dateien automatisch aufnehmen' });
  check('Einstellungen: "automatisch aufnehmen" ist ohne "nach dem Start" gesperrt', await autoAdd.isDisabled());
  await onStart.click();
  await page.waitForTimeout(600);
  await autoAdd.click();
  await page.waitForTimeout(800);
  await shotAll(page, 'einstellungen-abgleich', { withAxe: false });
  check('Einstellungen: beide Schalter an', await onStart.getAttribute('aria-checked') === 'true' && await autoAdd.getAttribute('aria-checked') === 'true');
  check('Einstellungen: Server hat beide gespeichert (get_sync_on_start / get_sync_auto_add)', await api.invoke('get_sync_on_start') === true && await api.invoke('get_sync_auto_add') === true);
  await page.reload();
  await page.getByRole('button', { name: 'Einstellungen' }).first().click();
  await page.getByRole('tab', { name: 'Katalog' }).click();
  check('Einstellungen: nach Neuladen weiter an', await page.getByRole('switch', { name: 'Katalogordner nach dem Start abgleichen' }).getAttribute('aria-checked') === 'true' && await page.getByRole('switch', { name: 'Neue Dateien automatisch aufnehmen' }).getAttribute('aria-checked') === 'true');
  await browser.close();
};

// ---- phase: startup (after the container restart with both switches on and 3 new files on disk) ----------------------
phaseFns.startup = async () => {
  const api = await new Api().login();
  let r = await api.invoke('run_start_sync');
  for (let i = 0; i < 60 && r.busy; i++) { await sleep(1000); r = await api.invoke('run_start_sync'); }
  note('run_start_sync: ' + JSON.stringify(r));
  check('Start-Abgleich lief einmal (ran, state=done)', r.ran === true && r.state === 'done', JSON.stringify(r));
  check('Start-Abgleich hat 3 neue Dateien aufgenommen', r.applied?.added === 3, JSON.stringify(r.applied));
  check('Start-Abgleich meldet die noch fehlenden Modelle (2), entfernt nichts', r.missingFiles === 2, `missingFiles=${r.missingFiles}`);
  const browser = await chromium.launch();
  const page = await newPage(browser);
  await login(page);
  const banner = page.getByRole('status').filter({ hasText: /Beim Start/ });
  await banner.waitFor({ timeout: 30000 });
  const text = (await banner.innerText()).replace(/\s+/g, ' ');
  note('Hinweisleiste: ' + text);
  check('Hinweisleiste nach der Anmeldung: "Beim Start aufgenommen: 3 neue Dateien." mit Zahlen', /aufgenommen: 3 neue Dateien/.test(text), text);
  check('Hinweisleiste nennt die weiter offenen fehlenden Modelle', /Weiter offen: 2 Modelle fehlen/.test(text) || /Weiter offen/.test(text), text);
  await shotAll(page, 'startbanner', { withAxe: false });
  const bannerAxe = await new AxeBuilder({ page }).include('[role="status"]').analyze();
  check('axe Hinweisleiste: keine serious/critical', bannerAxe.violations.filter(v => v.impact === 'serious' || v.impact === 'critical').length === 0, bannerAxe.violations.map(v => v.id).join(','));
  await banner.getByRole('button', { name: 'Ansehen' }).click();
  await waitStep(page, 'select');
  check('Hinweisleiste: "Ansehen" öffnet den Abgleich-Dialog', true);
  const t = (await dialog(page).innerText()).replace(/\s+/g, ' ');
  check('Abgleich nach dem Start-Lauf: 0 neue Dateien, die 2 fehlenden Modelle weiter vorhanden (nichts automatisch entfernt)', /0 neue Dateien/.test(t) && /2 Modelle fehlen/.test(t), t.slice(0, 160));
  await page.keyboard.press('Escape');
  await browser.close();
};

// ---- phase: emptyroot (P5e: a root emptied behind the catalog's back, holding only the server's own trash folder) -----
phaseFns.emptyroot = async () => {
  const api = await new Api().login();
  let r = await api.invoke('run_start_sync');
  for (let i = 0; i < 60 && r.busy; i++) { await sleep(1000); r = await api.invoke('run_start_sync'); }
  note('emptyroot run_start_sync: ' + JSON.stringify(r));
  const report = await api.invoke('disk_sync_report');
  note('emptyroot disk_sync_report roots: ' + JSON.stringify(report.roots) + ` fehlt=${report.missingModels.length} unklar=${(report.unclear ?? []).length}`);
  check('Leere Wurzel mit .katalog-papierkorb: Start-Abgleich zählt sie als nicht erreichbar (unreachableRoots >= 1)', r.unreachableRoots >= 1, `unreachableRoots=${r.unreachableRoots}`);
  check('Leere Wurzel: Bericht stuft sie nicht als "ok" ein und listet ihre Modelle nicht als fehlend', report.roots.find(x => x.name === 'beta')?.status !== 'ok', JSON.stringify(report.roots));
  check('Leere Wurzel: nichts wurde aufgenommen oder angewendet', !r.applied || (r.applied.added === 0 && r.applied.renamed === 0), JSON.stringify(r.applied));
};

// ---- phase: cleanupempty (P6c: everything removed in the dialog -> confirmation, empty state, "Schließen") ---------------
phaseFns.cleanupempty = async () => {
  const browser = await chromium.launch();
  const page = await newPage(browser);
  await login(page);
  await page.waitForTimeout(1500);
  const trigger = page.getByRole('button', { name: 'Aufräum-Vorschläge' });
  await trigger.click();
  await dialog(page).waitFor();
  await page.waitForTimeout(800);
  const dlg = dialog(page);
  let removed = 0;
  for (let i = 0; i < 8 && !/Nichts mehr zu bereinigen/.test(await dlg.innerText()); i++) {
    const boxes = dlg.locator('input[type=checkbox]');
    for (let b = 0; b < await boxes.count(); b++) if (await boxes.nth(b).isEnabled() && !(await boxes.nth(b).isChecked())) await boxes.nth(b).check();
    const btn = dlg.getByRole('button', { name: /^Auswahl in den Papierkorb$/ });
    const folder = dlg.getByRole('button', { name: /^\d+ Modelle? in den Papierkorb$/ }).first();
    if (await btn.isEnabled()) await btn.click(); else if (await folder.count()) await folder.click(); else break;
    await page.getByText(/in den Papierkorb verschoben/).first().waitFor({ timeout: 20000 }).catch(() => {});
    removed++;
    await sleep(500);
    check(`Bereinigung leeren, Runde ${removed}: Dialog bleibt offen`, await page.locator('[role="dialog"]').count() === 1);
    const f = await focusInfo(page);
    check(`Bereinigung leeren, Runde ${removed}: Fokus im Dialog, nicht auf body`, f.inDialog, f.name);
  }
  const text = (await dlg.innerText()).replace(/\s+/g, ' ');
  note('Bereinigung, Leerzustand zeigt: ' + text.slice(0, 400));
  check('Bereinigung: Bestätigung "N Modelle in den Papierkorb verschoben" sichtbar', /\d+ Modelle? in den Papierkorb verschoben/.test(text), text.slice(0, 160));
  check('Bereinigung: Leerzustand "Nichts mehr zu bereinigen"', /Nichts mehr zu bereinigen/.test(text), text.slice(0, 160));
  check('Bereinigung: Knopf heißt jetzt "Schließen" (kein "Abbrechen")', await dlg.getByRole('button', { name: 'Schließen', exact: true }).count() >= 1 && await dlg.getByRole('button', { name: 'Abbrechen', exact: true }).count() === 0);
  await shotAll(page, 'empty-state', { withAxe: false });
  await axeAllViews(page, 'empty-state (Leerzustand)');
  await dlg.getByRole('button', { name: 'Schließen', exact: true }).first().click();
  await page.locator('[role="dialog"]').waitFor({ state: 'detached', timeout: 5000 }).catch(() => {});
  const back = await focusInfo(page);
  check('Bereinigung: nach "Schließen" kehrt der Fokus zum Auslöser zurück', back.name.includes('Aufräum-Vorschläge'), back.name);
  // reopen: nothing left to clean up, still the same wording without the confirmation
  await trigger.click();
  await dialog(page).waitFor();
  await page.waitForTimeout(800);
  const again = (await dialog(page).innerText()).replace(/\s+/g, ' ');
  check('Bereinigung: erneut geöffnet ohne Bestätigung, mit "Keine Probleme"-Text', !/verschoben/.test(again), again.slice(0, 160));
  await browser.close();
};

await phaseFns[phase]?.();
const total = results.length;
writeFileSync(`${out}/results-${phase}.json`, JSON.stringify({ phase, pass, total, results, findings }, null, 1));
console.log(`PASS ${pass}/${total}`);
process.exit(pass === total ? 0 : 1);
