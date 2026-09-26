import type { Language } from '../i18n/types';
import type { TagCount } from '../types';
import AUTO_TAGS from './autoTags.json';

// Name table of the automatic tags, shared with the Rust backend
// (src-tauri/src/tagging.rs reads the same JSON file). The identifier is
// the German name and is stored like that in the database; only the
// display is translated here.
type AutoTagNames = Record<Language, string>;
const TABLE: Record<string, AutoTagNames> = AUTO_TAGS;

function aliasKey(name: string): string {
  return name.trim().toLowerCase();
}

const ALIAS_TO_CANONICAL = new Map<string, string>();
for (const [canonical, names] of Object.entries(TABLE)) {
  for (const name of Object.values(names)) ALIAS_TO_CANONICAL.set(aliasKey(name), canonical);
}

export function tagLabel(tag: string, language: Language): string {
  // hasOwnProperty instead of index access: "constructor" etc. would otherwise
  // return an inherited property (Object.hasOwn needs ES2022, target is ES2020).
  return Object.prototype.hasOwnProperty.call(TABLE, tag) ? TABLE[tag][language] : tag;
}

export function canonicalTag(input: string): string {
  return ALIAS_TO_CANONICAL.get(aliasKey(input)) ?? input;
}

export function tagMatches(tag: string, query: string, language: Language): boolean {
  const q = query.toLowerCase();
  return tag.toLowerCase().includes(q) || tagLabel(tag, language).toLowerCase().includes(q);
}

export function sortTagsForDisplay(tags: TagCount[], language: Language): TagCount[] {
  return [...tags].sort((a, b) => tagLabel(a.label, language).localeCompare(tagLabel(b.label, language), language));
}
