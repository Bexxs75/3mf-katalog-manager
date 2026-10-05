import { readFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { E2E_ROOT, OUT_DIR } from './run-env.js';

interface AllowEntry { selector: string; reason: string }
export interface Allowlist { ellipsis: AllowEntry[]; clipped: AllowEntry[]; fonts: AllowEntry[]; glyphs: AllowEntry[] }

const SCRIPT = readFileSync(join(E2E_ROOT, 'lib', 'browser-audit.js'), 'utf8');

/** Text characters that must never reach the screen as glyphs; the app uses SVG icons instead. */
export const FORBIDDEN_GLYPHS = ['✓', '✕', '▾', '▴', '♥', '♡', '⌕', '✎', '＋', '⇄', '⚠', '📦', '⚖'];

export function loadAllowlist(): Allowlist {
  const list = JSON.parse(readFileSync(join(E2E_ROOT, 'layout-allowlist.json'), 'utf8')) as Record<string, unknown>;
  for (const key of ['ellipsis', 'clipped', 'fonts', 'glyphs']) {
    for (const entry of (list[key] as AllowEntry[]) ?? []) {
      if (!entry.selector || !entry.reason || entry.reason.trim().length < 10) {
        throw new Error(`layout-allowlist.json: entry in "${key}" needs a selector and a real reason (${JSON.stringify(entry)})`);
      }
    }
  }
  return list as unknown as Allowlist;
}

export interface Problem { check: string; element: string; text: string; [k: string]: unknown }

export async function runAudit(kind: 'layout' | 'typography', extra: Record<string, unknown> = {}): Promise<Problem[]> {
  const allow = loadAllowlist();
  return browser.execute(`${SCRIPT}\nreturn __mfkAudit(arguments[0], arguments[1]);`, kind, { allow, glyphs: FORBIDDEN_GLYPHS, ...extra }) as Promise<Problem[]>;
}

/** Fails with element, text and sizes of every problem and leaves a screenshot in e2e/out/. */
export async function expectNoProblems(problems: Problem[], label: string) {
  if (problems.length === 0) return;
  mkdirSync(join(OUT_DIR, 'screenshots'), { recursive: true });
  const file = join(OUT_DIR, 'screenshots', `${label.replace(/[^\w.-]+/g, '_')}.png`);
  await browser.saveScreenshot(file);
  const lines = problems.slice(0, 25).map((p) => `  - [${p.check}] <${p.element}> "${p.text}" ${JSON.stringify({ ...p, check: undefined, element: undefined, text: undefined })}`);
  throw new Error(`${problems.length} problem(s) in ${label} (screenshot: ${file})\n${lines.join('\n')}`);
}
