import type { ModelFile } from '../types';

// Werkzeug-Ansichten der Seitenleiste: rein berechnet aus der Modellliste,
// ohne Seiteneffekte, damit sie einzeln testbar sind.
export type ToolView = 'recent' | 'new' | 'duplicates';

export const RECENT_LIMIT = 20;
export const NEW_WINDOW_DAYS = 7;
const DAY_MS = 24 * 60 * 60 * 1000;

export interface ToolCounts {
  recent: number;
  new: number;
  duplicateGroups: number;
}

export const TOOL_VIEW_LABEL_KEY: Record<ToolView, 'toolRecent' | 'toolNew' | 'toolDuplicates'> = {
  recent: 'toolRecent',
  new: 'toolNew',
  duplicates: 'toolDuplicates',
};

function time(value: string | null): number {
  if (!value) return Number.NaN;
  return Date.parse(value);
}

function recentModels(models: ModelFile[]): ModelFile[] {
  return models
    .filter((m) => !Number.isNaN(time(m.lastViewedAt)))
    .sort((a, b) => time(b.lastViewedAt) - time(a.lastViewedAt))
    .slice(0, RECENT_LIMIT);
}

function newModels(models: ModelFile[], now: Date): ModelFile[] {
  const limit = NEW_WINDOW_DAYS * DAY_MS;
  return models
    .filter((m) => {
      const t = time(m.importedAt);
      return !Number.isNaN(t) && now.getTime() - t <= limit;
    })
    .sort((a, b) => time(b.importedAt) - time(a.importedAt));
}

function duplicateGroups(models: ModelFile[]): ModelFile[][] {
  const byHash = new Map<string, ModelFile[]>();
  for (const m of models) {
    if (!m.contentHash) continue;
    const group = byHash.get(m.contentHash);
    if (group) group.push(m);
    else byHash.set(m.contentHash, [m]);
  }
  return [...byHash.values()]
    .filter((g) => g.length >= 2)
    .map((g) => [...g].sort((a, b) => time(a.importedAt) - time(b.importedAt)))
    .sort((a, b) => a[0].name.localeCompare(b[0].name));
}

export function applyToolView(models: ModelFile[], view: ToolView, now: Date): ModelFile[] {
  switch (view) {
    case 'recent':
      return recentModels(models);
    case 'new':
      return newModels(models, now);
    case 'duplicates':
      return duplicateGroups(models).flat();
  }
}

export function toolCounts(models: ModelFile[], now: Date): ToolCounts {
  return {
    recent: recentModels(models).length,
    new: newModels(models, now).length,
    duplicateGroups: duplicateGroups(models).length,
  };
}
