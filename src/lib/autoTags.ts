import type { Language } from '../i18n/types';
import type { TagCount } from '../types';
import AUTO_TAGS from './autoTags.json';

// Namentabelle der automatischen Tags, gemeinsam mit dem Rust-Backend
// (src-tauri/src/tagging.rs liest dieselbe JSON-Datei). Die Kennung ist
// der deutsche Name und steht so in der Datenbank; hier wird nur die
// Anzeige uebersetzt.
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
  // hasOwnProperty statt Index-Zugriff: "constructor" o.ae. lieferte sonst ein
  // geerbtes Property (Object.hasOwn braucht ES2022, Ziel ist ES2020).
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
