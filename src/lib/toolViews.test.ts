import { describe, expect, it } from 'vitest';
import { applyToolView, toolCounts, RECENT_LIMIT, TOOL_VIEW_LABEL_KEY } from './toolViews';
import { makeModelFile } from '../test/factories';

const NOW = new Date('2026-09-24T12:00:00.000Z');
const daysAgo = (d: number, extraMs = 0) => new Date(NOW.getTime() - d * 86_400_000 - extraMs).toISOString();
const ids = (ms: { id: string }[]) => ms.map((m) => m.id);

describe('applyToolView recent', () => {
  it('keeps only viewed models, newest first', () => {
    const models = [
      makeModelFile({ id: 'a', lastViewedAt: daysAgo(3) }),
      makeModelFile({ id: 'b', lastViewedAt: null }),
      makeModelFile({ id: 'c', lastViewedAt: daysAgo(1) }),
    ];
    expect(ids(applyToolView(models, 'recent', NOW))).toEqual(['c', 'a']);
  });

  it('limits to the 20 most recently viewed', () => {
    const models = Array.from({ length: RECENT_LIMIT + 1 }, (_, i) =>
      makeModelFile({ id: `m${i}`, lastViewedAt: daysAgo(i) }),
    );
    const result = applyToolView(models, 'recent', NOW);
    expect(result).toHaveLength(20);
    expect(result[0].id).toBe('m0');
    expect(ids(result)).not.toContain('m20');
  });
});

describe('applyToolView new', () => {
  it('includes imports within 7 days (inclusive), newest first', () => {
    const models = [
      makeModelFile({ id: 'edge', importedAt: daysAgo(7) }),
      makeModelFile({ id: 'old', importedAt: daysAgo(7, 60_000) }),
      makeModelFile({ id: 'fresh', importedAt: daysAgo(0, 1000) }),
    ];
    expect(ids(applyToolView(models, 'new', NOW))).toEqual(['fresh', 'edge']);
  });

  it('ignores unparsable dates', () => {
    const models = [makeModelFile({ id: 'x', importedAt: 'kaputt' })];
    expect(applyToolView(models, 'new', NOW)).toEqual([]);
  });
});

describe('applyToolView duplicates', () => {
  it('keeps only hashes with at least two models, grouped, oldest first, groups by first name', () => {
    const models = [
      makeModelFile({ id: 'z2', name: 'Zahnrad Kopie', contentHash: 'h2', importedAt: daysAgo(1) }),
      makeModelFile({ id: 'single', name: 'Einzeln', contentHash: 'h3', importedAt: daysAgo(1) }),
      makeModelFile({ id: 'a2', name: 'Adapter neu', contentHash: 'h1', importedAt: daysAgo(1) }),
      makeModelFile({ id: 'z1', name: 'Zahnrad', contentHash: 'h2', importedAt: daysAgo(5) }),
      makeModelFile({ id: 'nohash', name: 'Ohne Hash', contentHash: null }),
      makeModelFile({ id: 'a1', name: 'Adapter', contentHash: 'h1', importedAt: daysAgo(9) }),
    ];
    expect(ids(applyToolView(models, 'duplicates', NOW))).toEqual(['a1', 'a2', 'z1', 'z2']);
  });
});

describe('toolCounts', () => {
  it('counts recent (capped), new and duplicate groups', () => {
    const models = [
      makeModelFile({ id: '1', lastViewedAt: daysAgo(1), importedAt: daysAgo(2), contentHash: 'h' }),
      makeModelFile({ id: '2', lastViewedAt: null, importedAt: daysAgo(30), contentHash: 'h' }),
      makeModelFile({ id: '3', lastViewedAt: null, importedAt: daysAgo(30), contentHash: 'x' }),
    ];
    expect(toolCounts(models, NOW)).toEqual({ recent: 1, new: 1, duplicateGroups: 1 });
  });

  it('reports zero duplicate groups when all hashes are unique', () => {
    const models = [makeModelFile({ id: '1', contentHash: 'a' }), makeModelFile({ id: '2', contentHash: 'b' })];
    expect(toolCounts(models, NOW).duplicateGroups).toBe(0);
  });
});

describe('TOOL_VIEW_LABEL_KEY', () => {
  it('maps every view to its i18n key', () => {
    expect(TOOL_VIEW_LABEL_KEY).toEqual({ recent: 'toolRecent', new: 'toolNew', duplicates: 'toolDuplicates' });
  });
});
