import type { ModelFile } from '../types';

// Werkzeug-Ansichten der Seitenleiste: rein berechnet aus der Modellliste,
// ohne Seiteneffekte, damit sie einzeln testbar sind.
export type ToolView = 'recent' | 'new' | 'favorites' | 'duplicates';

export const RECENT_LIMIT = 20;
export const NEW_WINDOW_DAYS = 7;
const DAY_MS = 24 * 60 * 60 * 1000;

export interface ToolCounts {
  recent: number;
  new: number;
  favorites: number;
  duplicateGroups: number;
}

export const TOOL_VIEW_LABEL_KEY: Record<ToolView, 'toolRecent' | 'toolNew' | 'toolFavorites' | 'toolDuplicates'> = {
  recent: 'toolRecent',
  new: 'toolNew',
  favorites: 'toolFavorites',
  duplicates: 'toolDuplicates',
};

// Eingefrorener Stand von lastViewedAt je Modell-id, aufgenommen beim
// (Re-)Aktivieren der "recent"-Ansicht - haelt die Reihenfolge stabil,
// waehrend die Ansicht aktiv ist (siehe useCatalogFilters).
export type RecentSnapshot = Map<string, string | null>;

function time(value: string | null): number {
  if (!value) return Number.NaN;
  return Date.parse(value);
}

function recentModels(models: ModelFile[], snapshot?: RecentSnapshot): ModelFile[] {
  const timestampOf = snapshot ? (m: ModelFile) => snapshot.get(m.id) ?? null : (m: ModelFile) => m.lastViewedAt;
  return models
    .map((m) => ({ t: time(timestampOf(m)), m }))
    .filter((entry) => !Number.isNaN(entry.t))
    .sort((a, b) => b.t - a.t)
    .slice(0, RECENT_LIMIT)
    .map((entry) => entry.m);
}

function newModels(models: ModelFile[], now: Date): ModelFile[] {
  const limit = NEW_WINDOW_DAYS * DAY_MS;
  const nowMs = now.getTime();
  return models
    .map((m) => ({ t: time(m.importedAt), m }))
    .filter((entry) => !Number.isNaN(entry.t) && nowMs - entry.t <= limit)
    .sort((a, b) => b.t - a.t)
    .map((entry) => entry.m);
}

function favoriteModels(models: ModelFile[]): ModelFile[] {
  return models.filter((m) => m.favorite).sort((a, b) => a.name.localeCompare(b.name));
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
    .map((g) =>
      g
        .map((m) => ({ t: time(m.importedAt), m }))
        .sort((a, b) => a.t - b.t)
        .map((entry) => entry.m),
    )
    .sort((a, b) => a[0].name.localeCompare(b[0].name));
}

export function applyToolView(
  models: ModelFile[],
  view: ToolView,
  now: Date,
  recentSnapshot?: RecentSnapshot,
): ModelFile[] {
  switch (view) {
    case 'recent':
      return recentModels(models, recentSnapshot);
    case 'new':
      return newModels(models, now);
    case 'favorites':
      return favoriteModels(models);
    case 'duplicates':
      return duplicateGroups(models).flat();
  }
}

export function toolCounts(models: ModelFile[], now: Date): ToolCounts {
  const recentCount = models.reduce((acc, m) => (Number.isNaN(time(m.lastViewedAt)) ? acc : acc + 1), 0);
  return {
    recent: Math.min(RECENT_LIMIT, recentCount),
    new: newModels(models, now).length,
    favorites: models.reduce((acc, m) => (m.favorite ? acc + 1 : acc), 0),
    duplicateGroups: duplicateGroups(models).length,
  };
}
