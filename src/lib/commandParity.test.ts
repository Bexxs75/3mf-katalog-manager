import { expect, it } from 'vitest';
import tauriLib from '../../src-tauri/src/lib.rs?raw';
import invokeSource from '../../crates/katalog-server/src/invoke.rs?raw';
import writeSource from '../../crates/katalog-server/src/write.rs?raw';
import classes from '../../docs/web/command-classes.json';
import { WEB_READ_COMMANDS, WEB_WRITE_COMMANDS } from './readOnly';

// A desktop command that is not classified for the web is a gap nobody decided on. This test makes the decision visible:
// a new command in generate_handler! stays red until docs/web/command-classes.json says web, B (web variant feasible) or C (not transferable).
const handler = (tauriLib as string).split('generate_handler!')[1].split(']')[0];
const desktop = [...handler.matchAll(/(?:\b[A-Za-z_][A-Za-z0-9_]*::)*\b([a-z][a-z0-9_]*)\s*,/g)].map(m => m[1]);
const rustList = (source: string, name: string) => [...(source.split(`pub const ${name}: &[&str] = &[`)[1].split('];')[0]).matchAll(/"([a-z0-9_]+)"/g)].map(m => m[1]);
const classOf = classes.commands as Record<string, string>;

it('finds the desktop command list', () => {
  expect(desktop.length).toBeGreaterThan(100);
  expect(new Set(desktop).size).toBe(desktop.length);
});

it('classifies every desktop command as web, B or C and nothing else', () => {
  const unclassified = desktop.filter(command => !classOf[command]);
  expect(unclassified, 'Add these to docs/web/command-classes.json as web, B or C').toEqual([]);
  const stale = Object.entries(classOf).filter(([command, kind]) => kind !== 'web-only' && !desktop.includes(command)).map(([command]) => command);
  expect(stale, 'Commands that no longer exist on the desktop').toEqual([]);
  expect(Object.values(classOf).every(kind => ['web', 'web-only', 'B', 'C'].includes(kind))).toBe(true);
});

it('keeps the classification identical to what the web allows (TypeScript and Rust lists)', () => {
  const allowed = new Set<string>([...WEB_READ_COMMANDS, ...WEB_WRITE_COMMANDS]);
  const rustAllowed = new Set<string>([...rustList(invokeSource as string, 'READ_COMMANDS'), ...rustList(writeSource as string, 'WRITE_COMMANDS')]);
  expect([...allowed].sort()).toEqual([...rustAllowed].sort());
  const webClass = Object.entries(classOf).filter(([, kind]) => kind === 'web' || kind === 'web-only').map(([command]) => command).sort();
  expect(webClass).toEqual([...allowed].sort());
  const webOnlyOnDesktop = Object.entries(classOf).filter(([command, kind]) => kind === 'web-only' && desktop.includes(command)).map(([command]) => command);
  expect(webOnlyOnDesktop, 'web-only commands must not exist on the desktop').toEqual([]);
});
