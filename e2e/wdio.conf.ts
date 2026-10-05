import { cpSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { DRIVER_PORT, startDriver, stopDriver, waitForAppGone } from './lib/driver.js';
import { appUnderTest, isolatedEnv, OUT_DIR } from './lib/run-env.js';
import { ensureTemplate, seedScratch, type Scratch } from './lib/seed.js';

let current: Scratch | null = null;

export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.e2e.ts'],
  maxInstances: 1, // one app at a time: the release build allows a single instance
  hostname: '127.0.0.1',
  port: DRIVER_PORT,
  path: '/',
  logLevel: 'warn',
  capabilities: [{
    browserName: 'wry',
    // tauri-driver does not speak WebDriver BiDi.
    'wdio:enforceWebDriverClassic': true,
    'tauri:options': { application: process.env.MFK_E2E_APP ?? '' },
  } as WebdriverIO.Capabilities],
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: { ui: 'bdd', timeout: 120_000 },
  waitforTimeout: 15_000,

  async onPrepare() {
    mkdirSync(OUT_DIR, { recursive: true });
    appUnderTest();
    await ensureTemplate();
  },

  async beforeSession(_config, _caps, specs) {
    const label = (specs[0] ?? 'spec').split(/[\\/]/).pop()!.replace(/\.e2e\.ts$/, '');
    await waitForAppGone(appUnderTest());
    current = seedScratch(label);
    process.env.MFK_E2E_HOME = current.home;
    process.env.MFK_E2E_CATALOG = current.catalogDir;
    process.env.MFK_E2E_EXTRAS = current.extrasDir;
    await startDriver(isolatedEnv(current.home));
  },

  async afterTest(test, _context, { passed }) {
    if (passed) return;
    mkdirSync(join(OUT_DIR, 'screenshots'), { recursive: true });
    const name = `${test.parent} ${test.title}`.replace(/[^\w.-]+/g, '_').slice(0, 120);
    try { await browser.saveScreenshot(join(OUT_DIR, 'screenshots', `${name}.png`)); } catch { /* window may be gone */ }
  },

  async afterSession(_config, _caps, specs) {
    await stopDriver();
    if (current) {
      // Keep the app's own log next to the screenshots, it explains most failures.
      // Linux logs next to the data; on Windows the log plugin writes below %LOCALAPPDATA%.
      const identifier = basename(current.appDataDir);
      const candidates = [join(current.appDataDir, 'logs'), ...(process.env.LOCALAPPDATA ? [join(process.env.LOCALAPPDATA, identifier, 'logs')] : [])];
      const logs = candidates.find((dir) => existsSync(dir));
      const label = (specs[0] ?? 'spec').split(/[\\/]/).pop()!.replace(/\.e2e\.ts$/, '');
      if (logs) cpSync(logs, join(OUT_DIR, 'app-logs', label), { recursive: true });
      else writeFileSync(join(OUT_DIR, `${label}.nolog`), '');
    }
    await waitForAppGone(appUnderTest());
  },
};
