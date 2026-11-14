import { describe, expect, it } from 'vitest';
import gold from './printStatusGroups.json';
import { printStatusGroup } from './printStatusGroups';

// The Rust side reads the same file (katalog-core printer_link/failure.rs).
describe('printStatusGroup', () => {
  it.each(gold.cases.map(c => [c.rawStatus, c.reason, c.group] as const))('%j with reason %j is %s', (rawStatus, reason, group) => {
    expect(printStatusGroup(rawStatus, reason)).toBe(group);
  });

  it('only names groups the table lists', () => {
    for (const c of gold.cases) expect(gold.groups).toContain(c.group);
    for (const group of Object.values(gold.statusGroups)) expect(gold.groups).toContain(group);
  });

  it('does not take a status named like an object property for a known one', () => {
    expect(printStatusGroup('constructor', null)).toBe('other');
    expect(printStatusGroup('__proto__', null)).toBe('other');
  });
});
