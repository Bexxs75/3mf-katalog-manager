import type { Bundle, BundleKind, ModelFile } from '../types';
import { splitFileName } from './fileName';
import { MAX_BUNDLE_MEMBERS, MIN_BUNDLE_MEMBERS, indexBundles } from './bundles';

/** Why a group of files is offered as a bundle; the signal decides whether the kind is already known. */
export type SuggestionReason = 'sameModelId' | 'similarName';

export interface BundleSuggestion {
  /** Stable key of the group (the signal, not the members), so a dismissal survives new files in the group. */
  key: string;
  reason: SuggestionReason;
  /** Platform and model id for `sameModelId`; the folder id for `similarName`. */
  origin: { platform: string; modelId: string } | { folderId: string };
  /** Proposed bundle name; the user can change it. */
  name: string;
  /** `variants` for one platform model with several profiles; open (null) for a similar name. */
  kind: BundleKind | null;
  /** Files that are not in a bundle yet, in name order (the first one becomes the title model). */
  fileIds: string[];
}

/**
 * Endings that mark a numbered copy, part or version of one model. Each entry is tried in turn and
 * the name is shortened as long as one of them matches ("Ship v2 part 1" -> "ship"). The
 * look-behind (not `\b`) is deliberate: an underscore counts as a word character for `\b`.
 */
export const NAME_STEM_ENDINGS: { label: string; pattern: RegExp }[] = [
  { label: 'part word and number', pattern: /[\s_\-.]*(?<![\p{L}\p{N}])(?:teil|part|pt|tl|parte|partie)[\s_\-.]*\d+$/iu },
  { label: 'version', pattern: /[\s_\-.]*(?<![\p{L}\p{N}])v(?:er(?:sion)?)?[\s_\-.]*\d+(?:\.\d+)*$/iu },
  { label: 'number in brackets', pattern: /[\s_\-.]*\(\d+\)$/ },
  { label: 'copy word', pattern: /[\s_\-.]*(?<![\p{L}\p{N}])(?:copy|kopie|copia|copie)(?:[\s_\-.]*\d+)?$/iu },
  { label: 'closing digits', pattern: /[\s_\-.]*\d+$/ },
];

/** Shortest stem that still says something; "1" or "ab" would match unrelated files. */
const MIN_STEM_CHARS = 3;

/**
 * The name without extension and without the endings of `NAME_STEM_ENDINGS`, lower-cased and with
 * runs of separators collapsed. Empty when nothing meaningful is left.
 */
export function nameStem(fileName: string): string {
  let stem = splitFileName(fileName).base.normalize('NFC').trim();
  for (let again = true; again;) {
    again = false;
    for (const { pattern } of NAME_STEM_ENDINGS) {
      const shorter = stem.replace(pattern, '');
      if (shorter !== stem) { stem = shorter; again = true; break; }
    }
  }
  stem = stem.replace(/[\s_\-.]+/g, ' ').trim().toLowerCase();
  return stem.length >= MIN_STEM_CHARS ? stem : '';
}

function byName(a: ModelFile, b: ModelFile): number {
  return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' }) || a.id.localeCompare(b.id);
}

/** The name proposed for a group: the common stem as the first file writes it, else that file's own base name. */
function proposedName(files: ModelFile[]): string {
  const first = splitFileName(files[0].name).base.trim();
  const stem = nameStem(files[0].name);
  if (!stem || files.some(file => nameStem(file.name) !== stem)) return first;
  // Cut the first file's own spelling at the stem's length, so capitals and separators stay as written.
  let kept = '';
  let words = stem.replace(/ /g, '');
  for (const character of first) {
    if (words === '') break;
    if (character.toLowerCase() === words[0]) words = words.slice(1);
    kept += character;
  }
  return kept.replace(/[\s_\-.]+$/, '') || first;
}

/**
 * Groups of files that probably belong together. Nothing here changes anything: the user decides.
 * Signals, in this order (a file belongs to the first group that takes it):
 * 1. one platform model with several profile ids (the profiles of one design are variants),
 * 2. one name stem in one folder (the kind is left open: it can be parts or variants).
 * Files already in a bundle, and groups of fewer than two or more than the bundle limit, are skipped.
 * A shared folder and creator alone is not a signal: it matches far too many files.
 */
export function suggestBundles(models: ModelFile[], bundles: Bundle[]): BundleSuggestion[] {
  const bundled = indexBundles(bundles);
  const free = models.filter(model => !bundled.has(model.id)).sort(byName);
  const taken = new Set<string>();
  const out: BundleSuggestion[] = [];
  const fits = (count: number) => count >= MIN_BUNDLE_MEMBERS && count <= MAX_BUNDLE_MEMBERS;

  const byModel = new Map<string, ModelFile[]>();
  for (const file of free) {
    if (!file.sourcePlatform || !file.sourceModelId || !file.sourceProfileId) continue;
    const key = JSON.stringify([file.sourcePlatform, file.sourceModelId]);
    byModel.set(key, [...byModel.get(key) ?? [], file]);
  }
  for (const [groupKey, files] of byModel) {
    if (new Set(files.map(file => file.sourceProfileId)).size < 2 || !fits(files.length)) continue;
    files.forEach(file => taken.add(file.id));
    out.push({
      key: `model:${groupKey}`,
      reason: 'sameModelId',
      origin: { platform: files[0].sourcePlatform ?? '', modelId: files[0].sourceModelId ?? '' },
      name: proposedName(files),
      kind: 'variants',
      fileIds: files.map(file => file.id),
    });
  }

  const byStem = new Map<string, { folderId: string; stem: string; files: ModelFile[] }>();
  for (const file of free) {
    const stem = nameStem(file.name);
    if (!stem || taken.has(file.id)) continue;
    const key = JSON.stringify([file.folderId, stem]);
    const group = byStem.get(key) ?? { folderId: file.folderId, stem, files: [] };
    group.files.push(file);
    byStem.set(key, group);
  }
  for (const [groupKey, { folderId, files }] of byStem) {
    // Two copies of one name with no number are no "numbered model": only different names count.
    if (!fits(files.length) || new Set(files.map(file => file.name.toLowerCase())).size < MIN_BUNDLE_MEMBERS) continue;
    out.push({
      key: `stem:${groupKey}`,
      reason: 'similarName',
      origin: { folderId },
      name: proposedName(files),
      kind: null,
      fileIds: files.map(file => file.id),
    });
  }
  return out;
}

const STORAGE_KEY = '3mf-katalog-bundle-suggestions';
const MAX_REMEMBERED = 500;

export interface SuggestionPreferences {
  /** Whether a notice follows an import or a sync. The menu entry stays either way. */
  hint: boolean;
  /** Group keys the user asked not to be offered again. */
  dismissed: string[];
}

export function loadSuggestionPreferences(): SuggestionPreferences {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
    if (parsed && typeof parsed === 'object') {
      const { hint, dismissed } = parsed as Record<string, unknown>;
      return {
        hint: hint !== false,
        dismissed: Array.isArray(dismissed) ? dismissed.filter((key): key is string => typeof key === 'string') : [],
      };
    }
  } catch {
    // Unreadable storage or stale data: start from the defaults.
  }
  return { hint: true, dismissed: [] };
}

/** Comfort only: this is not catalog state, so a failure to store it is silent. */
export function saveSuggestionPreferences(preferences: SuggestionPreferences): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ hint: preferences.hint, dismissed: preferences.dismissed.slice(-MAX_REMEMBERED) }));
  } catch {
    // Storage blocked: the choice only applies to this session.
  }
}
