# Mehrsprachigkeit (i18n) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Anwendung auf vier Sprachen umschaltbar machen (Deutsch/Englisch/Spanisch/Französisch), manuell wählbar über das Einstellungen-Panel, ohne neue i18n-Bibliothek.

**Architecture:** Neues `src/i18n/`-Verzeichnis mit vier Wörterbüchern (`Translations`-Interface, compile-zeitlich erzwungen) und einem React-Context (`LanguageProvider`/`useLanguage()`/`useT()`), der nach dem Muster von `useTheme.ts` gebaut ist, aber Context statt Prop-Drilling nutzt. Backend liefert nach dem Umbau nur noch rohe, unformatierte Felder; alle Formatierung (Datum/Volumen/Dateigröße/relative Zeit) läuft im Frontend über `Intl`-APIs in `src/i18n/format.ts`.

**Tech Stack:** Tauri v2 (Rust/`rusqlite`), React 19 + TypeScript 6, Vite, Tailwind. Keine neue Bibliothek, keine neue Test-Infrastruktur.

## Global Constraints

- Keine automatische Spracherkennung/„System"-Option für Sprache (anders als Theme).
- Keine i18n-Bibliothek (react-i18next, react-intl) — custom Lösung.
- Keine neue automatisierte Test-Infrastruktur im Frontend (Projekt hat keinen Test-Runner; `package.json` enthält weder vitest noch jest).
- **Testmethode je Aufgabentyp** (aus Spec Abschnitt 4 abgeleitet): Für die Wörterbuch-Aufgaben (Task 1 Kernteil, Task 2) wird ein echter Rot/Grün-Zyklus über `npx tsc --noEmit` gefahren (Key entfernen → Fehler erwarten → Key wiederherstellen → Erfolg erwarten), weil das `Translations`-Interface fehlende Keys als Compile-Fehler erzwingt. Für alle reinen UI-String-Migrationsaufgaben (Task 4–11) ist `npx tsc --noEmit` (bzw. `npm run build` in Task 12) das alleinige Verifikationsmittel — das ist explizit die im Spec vorgesehene Absicherung ("fehlende Keys sind ein Compile-Fehler, kein Laufzeit-Fallback"), kein Ersatz für fehlende Tests. Manuelles Browser-Testing ist in Task 12 gebündelt.
- Backend-Rust-Tests: keine neuen Tests, nur Löschung der `format.rs`-Tests (Testanzahl sinkt von 30 auf 25) und Sicherstellen, dass `cargo build`/`cargo test` weiterhin grün sind.
- Bestehende Design-Tokens/Klassen (`segBase`/`segActive`/`segInactive`, CSS-Variablen aus `theme.css`) werden wiederverwendet, keine neuen Styles.
- Außerhalb des Scopes (nicht anfassen): das serverseitig generierte `"Alle Modelle"` in `commands.rs::list_folders` (Zeile ~147) und die hartcodierte `CLOUDS`-Konstante in `App.tsx` (Cloud-Anbieter-Namen/Quota-Strings) — beide sind nicht Teil der im Spec erfassten String-Inventur.
- Sprachnamen im Sprache-Umschalter ("Deutsch", "English", "Español", "Français") bleiben **immer im Original**, unabhängig von der aktiven UI-Sprache — hartcodierte Konstante, nicht über `t()` übersetzt.
- Backend-Fehlermeldungen (`Err(String)` aus Tauri-Commands) bleiben unangetastet — aktuell nirgends user-facing angezeigt.
- Git-Konventionen: explizites `git add <file1> <file2>` pro Datei (nie `-A`/`.`), Review via `git status --short`/`git diff` vor dem Stage, deutsche Commit-Messages via HEREDOC, Verifikation via `git log --oneline -N`, nie `--amend`.
- Nach jedem Task: kurze Zusammenfassung und Rücksprache mit dem User, bevor der nächste Task begonnen wird.

---

## File Structure

**Create:**
- `src/i18n/types.ts`
- `src/i18n/format.ts`
- `src/i18n/de.ts`
- `src/i18n/en.ts`
- `src/i18n/es.ts`
- `src/i18n/fr.ts`
- `src/i18n/LanguageContext.tsx`

**Modify:**
- `src-tauri/src/lib.rs`
- `src-tauri/src/commands.rs`
- `src/types/index.ts`
- `src/main.tsx`
- `src/components/Header.tsx`
- `src/components/Sidebar.tsx`
- `src/components/ModelGrid.tsx`
- `src/components/ModelList.tsx`
- `src/components/DetailPanel.tsx`
- `src/components/ContextMenu.tsx`
- `src/components/ModelViewer.tsx`

**Delete:**
- `src-tauri/src/format.rs`

---

## Task 1: i18n-Grundtypen, Formatierung und Referenz-Wörterbuch (Deutsch)

**Files:**
- Create: `src/i18n/types.ts`
- Create: `src/i18n/format.ts`
- Create: `src/i18n/de.ts`
- Test: manuelle `npx tsc --noEmit`-Verifikation (kein dediziertes Test-File)

**Interfaces:**
- Consumes: nichts (Fundament-Task)
- Produces:
  - `type Language = 'de' | 'en' | 'es' | 'fr'`
  - `interface PluralForms { one: string; other: string }`
  - `interface Translations { ...57 Keys... }` (vollständige Liste s. u.)
  - `function formatCount(forms: PluralForms, n: number): string`
  - `function formatBytes(bytes: number, language: Language): string`
  - `function formatVolumeCm3(volumeCm3: number | null, language: Language): string`
  - `function formatDimensions(dimensionsMm: [number, number, number] | null, language: Language): string`
  - `function formatDate(rfc3339: string, language: Language): string`
  - `function formatRelativeTime(rfc3339: string, language: Language): string`
  - `const de: Translations`

- [ ] **Step 1: `src/i18n/types.ts` schreiben**

```typescript
export type Language = 'de' | 'en' | 'es' | 'fr';

export interface PluralForms {
  one: string;
  other: string;
}

export interface Translations {
  import: string;
  importMoreOptionsAria: string;
  importFilesOption: string;
  importFolderOption: string;

  sortLabel: string;
  sortName: string;
  sortDate: string;
  sortSize: string;
  sortVolume: string;

  viewGrid: string;
  viewList: string;

  filesCount: PluralForms;

  settingsTitle: string;
  appearanceTitle: string;
  themeSystem: string;
  themeLight: string;
  themeDark: string;
  themeDescriptionSystem: string;
  themeDescriptionManual: string;
  languageTitle: string;

  searchPlaceholder: string;
  foldersHeading: string;
  tagsHeading: string;
  cloudAccountsHeading: string;
  cloudConnected: string;
  cloudError: string;
  cloudDisconnected: string;

  previewLabel3d: string;

  columnOrigin: string;
  columnName: string;
  columnTags: string;
  columnVolume: string;
  columnSize: string;
  columnSync: string;

  syncSynced: string;
  syncOutdated: string;
  syncLocalOnly: string;
  syncCloudOnly: string;

  cancel: string;
  delete: string;
  openInSlicer: string;
  deleteConfirmQuestion: string;
  deleteAriaLabel: string;

  emptyStateText: string;
  dragToRotate: string;
  metadataHeading: string;
  metaDimensions: string;
  metaVolume: string;
  metaObjectCount: string;
  metaMaterial: string;
  metaFileSize: string;
  metaImported: string;
  noValue: string;
  hashtagsHeading: string;
  addTagPlaceholder: string;

  loadingPreview: string;
  previewUnavailable: string;
}

export function formatCount(forms: PluralForms, n: number): string {
  const form = n === 1 ? forms.one : forms.other;
  return form.replace('{count}', String(n));
}
```

- [ ] **Step 2: `src/i18n/format.ts` schreiben**

```typescript
import type { Language } from './types';

const LOCALE_MAP: Record<Language, string> = {
  de: 'de-DE',
  en: 'en-US',
  es: 'es-ES',
  fr: 'fr-FR',
};

function localeFor(language: Language): string {
  return LOCALE_MAP[language];
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB'] as const;

export function formatBytes(bytes: number, language: Language): string {
  const locale = localeFor(language);
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < BYTE_UNITS.length - 1) {
    value /= 1024;
    unitIndex++;
  }
  const decimals = unitIndex < 2 ? 0 : 1;
  const formatted = new Intl.NumberFormat(locale, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  }).format(value);
  return `${formatted} ${BYTE_UNITS[unitIndex]}`;
}

export function formatVolumeCm3(volumeCm3: number | null, language: Language): string {
  if (volumeCm3 === null) return '–';
  const formatted = new Intl.NumberFormat(localeFor(language), {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  }).format(volumeCm3);
  return `${formatted} cm³`;
}

export function formatDimensions(
  dimensionsMm: [number, number, number] | null,
  language: Language,
): string {
  if (dimensionsMm === null) return '–';
  const nf = new Intl.NumberFormat(localeFor(language), { maximumFractionDigits: 0 });
  const [x, y, z] = dimensionsMm;
  return `${nf.format(x)} × ${nf.format(y)} × ${nf.format(z)} mm`;
}

export function formatDate(rfc3339: string, language: Language): string {
  const date = new Date(rfc3339);
  if (Number.isNaN(date.getTime())) return '–';
  return new Intl.DateTimeFormat(localeFor(language)).format(date);
}

export function formatRelativeTime(rfc3339: string, language: Language): string {
  const date = new Date(rfc3339);
  if (Number.isNaN(date.getTime())) return '–';

  const diffSeconds = Math.round((date.getTime() - Date.now()) / 1000);
  const absSeconds = Math.abs(diffSeconds);
  const rtf = new Intl.RelativeTimeFormat(localeFor(language), { numeric: 'auto' });

  if (absSeconds < 60) return rtf.format(0, 'second');
  if (absSeconds < 3600) return rtf.format(Math.round(diffSeconds / 60), 'minute');
  if (absSeconds < 86400) return rtf.format(Math.round(diffSeconds / 3600), 'hour');
  return rtf.format(Math.round(diffSeconds / 86400), 'day');
}
```

- [ ] **Step 3: `src/i18n/de.ts` schreiben**

```typescript
import type { Translations } from './types';

export const de: Translations = {
  import: 'Importieren',
  importMoreOptionsAria: 'Weitere Import-Optionen',
  importFilesOption: 'Dateien...',
  importFolderOption: 'Ordner...',

  sortLabel: 'Sortieren',
  sortName: 'Name',
  sortDate: 'Datum',
  sortSize: 'Dateigröße',
  sortVolume: 'Volumen',

  viewGrid: 'Raster',
  viewList: 'Liste',

  filesCount: { one: '{count} Datei', other: '{count} Dateien' },

  settingsTitle: 'Einstellungen',
  appearanceTitle: 'Erscheinungsbild',
  themeSystem: 'System',
  themeLight: 'Hell',
  themeDark: 'Dunkel',
  themeDescriptionSystem: 'Folgt automatisch der Systemeinstellung.',
  themeDescriptionManual: 'Manuell auf {mode} festgelegt.',
  languageTitle: 'Sprache',

  searchPlaceholder: 'Name oder Tag suchen …',
  foldersHeading: 'Ordner',
  tagsHeading: 'Tags',
  cloudAccountsHeading: 'Cloud-Konten',
  cloudConnected: 'verbunden',
  cloudError: 'Fehler',
  cloudDisconnected: 'getrennt',

  previewLabel3d: '3D Vorschau',

  columnOrigin: 'Herkunft',
  columnName: 'Name',
  columnTags: 'Tags',
  columnVolume: 'Volumen',
  columnSize: 'Größe',
  columnSync: 'Sync',

  syncSynced: 'Aktuell',
  syncOutdated: 'Veraltet',
  syncLocalOnly: 'Nur lokal',
  syncCloudOnly: 'Nur Cloud',

  cancel: 'Abbrechen',
  delete: 'Löschen',
  openInSlicer: 'In Slicer öffnen',
  deleteConfirmQuestion: 'Eintrag löschen?',
  deleteAriaLabel: 'Eintrag löschen',

  emptyStateText: 'Wähle ein Modell aus, um Details, Vorschau und Tags zu sehen.',
  dragToRotate: 'Ziehen zum Drehen',
  metadataHeading: 'Metadaten',
  metaDimensions: 'Größe',
  metaVolume: 'Volumen',
  metaObjectCount: 'Objekte',
  metaMaterial: 'Material',
  metaFileSize: 'Dateigröße',
  metaImported: 'Importiert',
  noValue: '–',
  hashtagsHeading: 'Hashtags',
  addTagPlaceholder: 'Tag hinzufügen',

  loadingPreview: 'Lädt Vorschau …',
  previewUnavailable: 'Vorschau nicht verfügbar',
};
```

- [ ] **Step 4: Verifizieren, dass alles kompiliert**

Run: `cd /home/thebexxs/Projekte/3mf-katalog-manager && npx tsc --noEmit`
Expected: PASS (keine Fehler — `de.ts` ist die einzige Datei, die `Translations` bisher implementiert, und tut das vollständig)

- [ ] **Step 5: Rot/Grün-Zyklus demonstrieren, dass das Interface Vollständigkeit erzwingt**

Entferne testweise die Zeile `delete: 'Löschen',` aus `src/i18n/de.ts`.

Run: `npx tsc --noEmit`
Expected: FAIL mit einer Fehlermeldung wie `Property 'delete' is missing in type '{ ... }' but required in type 'Translations'.`

Stelle die Zeile `delete: 'Löschen',` wieder her.

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/i18n/types.ts src/i18n/format.ts src/i18n/de.ts
git commit -m "$(cat <<'EOF'
feat: i18n-Grundtypen, Intl-Formatierung und deutsches Referenz-Wörterbuch

Translations-Interface erzwingt Vollständigkeit aller vier Sprachen zur
Compile-Zeit. format.ts ersetzt die spätere Löschung von src-tauri/src/format.rs
durch Intl-basierte, locale-abhängige Formatierung im Frontend.
EOF
)"
```

---

## Task 2: Übersetzungs-Wörterbücher Englisch, Spanisch, Französisch

**Files:**
- Create: `src/i18n/en.ts`
- Create: `src/i18n/es.ts`
- Create: `src/i18n/fr.ts`

**Interfaces:**
- Consumes: `Translations` aus `src/i18n/types.ts` (Task 1)
- Produces: `const en: Translations`, `const es: Translations`, `const fr: Translations`

- [ ] **Step 1: `src/i18n/en.ts` schreiben**

```typescript
import type { Translations } from './types';

export const en: Translations = {
  import: 'Import',
  importMoreOptionsAria: 'More import options',
  importFilesOption: 'Files...',
  importFolderOption: 'Folder...',

  sortLabel: 'Sort',
  sortName: 'Name',
  sortDate: 'Date',
  sortSize: 'File size',
  sortVolume: 'Volume',

  viewGrid: 'Grid',
  viewList: 'List',

  filesCount: { one: '{count} file', other: '{count} files' },

  settingsTitle: 'Settings',
  appearanceTitle: 'Appearance',
  themeSystem: 'System',
  themeLight: 'Light',
  themeDark: 'Dark',
  themeDescriptionSystem: 'Follows the system setting automatically.',
  themeDescriptionManual: 'Manually set to {mode}.',
  languageTitle: 'Language',

  searchPlaceholder: 'Search name or tag …',
  foldersHeading: 'Folders',
  tagsHeading: 'Tags',
  cloudAccountsHeading: 'Cloud accounts',
  cloudConnected: 'connected',
  cloudError: 'error',
  cloudDisconnected: 'disconnected',

  previewLabel3d: '3D preview',

  columnOrigin: 'Origin',
  columnName: 'Name',
  columnTags: 'Tags',
  columnVolume: 'Volume',
  columnSize: 'Size',
  columnSync: 'Sync',

  syncSynced: 'Synced',
  syncOutdated: 'Outdated',
  syncLocalOnly: 'Local only',
  syncCloudOnly: 'Cloud only',

  cancel: 'Cancel',
  delete: 'Delete',
  openInSlicer: 'Open in slicer',
  deleteConfirmQuestion: 'Delete entry?',
  deleteAriaLabel: 'Delete entry',

  emptyStateText: 'Select a model to see details, preview, and tags.',
  dragToRotate: 'Drag to rotate',
  metadataHeading: 'Metadata',
  metaDimensions: 'Size',
  metaVolume: 'Volume',
  metaObjectCount: 'Objects',
  metaMaterial: 'Material',
  metaFileSize: 'File size',
  metaImported: 'Imported',
  noValue: '–',
  hashtagsHeading: 'Hashtags',
  addTagPlaceholder: 'Add tag',

  loadingPreview: 'Loading preview …',
  previewUnavailable: 'Preview unavailable',
};
```

- [ ] **Step 2: Rot/Grün-Zyklus für `en.ts`**

Entferne testweise `viewList: 'List',` aus `src/i18n/en.ts`.

Run: `npx tsc --noEmit`
Expected: FAIL mit `Property 'viewList' is missing in type '{ ... }' but required in type 'Translations'.`

Stelle die Zeile wieder her.

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 3: `src/i18n/es.ts` schreiben**

```typescript
import type { Translations } from './types';

export const es: Translations = {
  import: 'Importar',
  importMoreOptionsAria: 'Más opciones de importación',
  importFilesOption: 'Archivos...',
  importFolderOption: 'Carpeta...',

  sortLabel: 'Ordenar',
  sortName: 'Nombre',
  sortDate: 'Fecha',
  sortSize: 'Tamaño de archivo',
  sortVolume: 'Volumen',

  viewGrid: 'Cuadrícula',
  viewList: 'Lista',

  filesCount: { one: '{count} archivo', other: '{count} archivos' },

  settingsTitle: 'Ajustes',
  appearanceTitle: 'Apariencia',
  themeSystem: 'Sistema',
  themeLight: 'Claro',
  themeDark: 'Oscuro',
  themeDescriptionSystem: 'Sigue automáticamente la configuración del sistema.',
  themeDescriptionManual: 'Fijado manualmente en {mode}.',
  languageTitle: 'Idioma',

  searchPlaceholder: 'Buscar nombre o etiqueta …',
  foldersHeading: 'Carpetas',
  tagsHeading: 'Etiquetas',
  cloudAccountsHeading: 'Cuentas en la nube',
  cloudConnected: 'conectado',
  cloudError: 'error',
  cloudDisconnected: 'desconectado',

  previewLabel3d: 'Vista previa 3D',

  columnOrigin: 'Origen',
  columnName: 'Nombre',
  columnTags: 'Etiquetas',
  columnVolume: 'Volumen',
  columnSize: 'Tamaño',
  columnSync: 'Sincro',

  syncSynced: 'Actualizado',
  syncOutdated: 'Desactualizado',
  syncLocalOnly: 'Solo local',
  syncCloudOnly: 'Solo nube',

  cancel: 'Cancelar',
  delete: 'Eliminar',
  openInSlicer: 'Abrir en el laminador',
  deleteConfirmQuestion: '¿Eliminar entrada?',
  deleteAriaLabel: 'Eliminar entrada',

  emptyStateText: 'Selecciona un modelo para ver detalles, vista previa y etiquetas.',
  dragToRotate: 'Arrastra para girar',
  metadataHeading: 'Metadatos',
  metaDimensions: 'Tamaño',
  metaVolume: 'Volumen',
  metaObjectCount: 'Objetos',
  metaMaterial: 'Material',
  metaFileSize: 'Tamaño de archivo',
  metaImported: 'Importado',
  noValue: '–',
  hashtagsHeading: 'Hashtags',
  addTagPlaceholder: 'Añadir etiqueta',

  loadingPreview: 'Cargando vista previa …',
  previewUnavailable: 'Vista previa no disponible',
};
```

- [ ] **Step 4: Rot/Grün-Zyklus für `es.ts`**

Entferne testweise `cancel: 'Cancelar',` aus `src/i18n/es.ts`.

Run: `npx tsc --noEmit`
Expected: FAIL mit `Property 'cancel' is missing in type '{ ... }' but required in type 'Translations'.`

Stelle die Zeile wieder her.

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 5: `src/i18n/fr.ts` schreiben**

```typescript
import type { Translations } from './types';

export const fr: Translations = {
  import: 'Importer',
  importMoreOptionsAria: "Plus d'options d'importation",
  importFilesOption: 'Fichiers...',
  importFolderOption: 'Dossier...',

  sortLabel: 'Trier',
  sortName: 'Nom',
  sortDate: 'Date',
  sortSize: 'Taille du fichier',
  sortVolume: 'Volume',

  viewGrid: 'Grille',
  viewList: 'Liste',

  filesCount: { one: '{count} fichier', other: '{count} fichiers' },

  settingsTitle: 'Paramètres',
  appearanceTitle: 'Apparence',
  themeSystem: 'Système',
  themeLight: 'Clair',
  themeDark: 'Sombre',
  themeDescriptionSystem: 'Suit automatiquement le paramètre système.',
  themeDescriptionManual: 'Défini manuellement sur {mode}.',
  languageTitle: 'Langue',

  searchPlaceholder: 'Rechercher un nom ou un tag …',
  foldersHeading: 'Dossiers',
  tagsHeading: 'Tags',
  cloudAccountsHeading: 'Comptes cloud',
  cloudConnected: 'connecté',
  cloudError: 'erreur',
  cloudDisconnected: 'déconnecté',

  previewLabel3d: 'Aperçu 3D',

  columnOrigin: 'Origine',
  columnName: 'Nom',
  columnTags: 'Tags',
  columnVolume: 'Volume',
  columnSize: 'Taille',
  columnSync: 'Sync',

  syncSynced: 'À jour',
  syncOutdated: 'Obsolète',
  syncLocalOnly: 'Local uniquement',
  syncCloudOnly: 'Cloud uniquement',

  cancel: 'Annuler',
  delete: 'Supprimer',
  openInSlicer: 'Ouvrir dans le slicer',
  deleteConfirmQuestion: "Supprimer l'entrée ?",
  deleteAriaLabel: "Supprimer l'entrée",

  emptyStateText: "Sélectionnez un modèle pour voir les détails, l'aperçu et les tags.",
  dragToRotate: 'Glisser pour faire pivoter',
  metadataHeading: 'Métadonnées',
  metaDimensions: 'Taille',
  metaVolume: 'Volume',
  metaObjectCount: 'Objets',
  metaMaterial: 'Matériau',
  metaFileSize: 'Taille du fichier',
  metaImported: 'Importé',
  noValue: '–',
  hashtagsHeading: 'Hashtags',
  addTagPlaceholder: 'Ajouter un tag',

  loadingPreview: "Chargement de l'aperçu …",
  previewUnavailable: 'Aperçu indisponible',
};
```

- [ ] **Step 6: Rot/Grün-Zyklus für `fr.ts`**

Entferne testweise `metaMaterial: 'Matériau',` aus `src/i18n/fr.ts`.

Run: `npx tsc --noEmit`
Expected: FAIL mit `Property 'metaMaterial' is missing in type '{ ... }' but required in type 'Translations'.`

Stelle die Zeile wieder her.

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "$(cat <<'EOF'
feat: Übersetzungs-Wörterbücher für Englisch, Spanisch, Französisch

Alle drei implementieren das Translations-Interface vollständig,
per tsc --noEmit verifiziert.
EOF
)"
```

---

## Task 3: LanguageContext (Provider + Hooks) und Einbindung in main.tsx

**Files:**
- Create: `src/i18n/LanguageContext.tsx`
- Modify: `src/main.tsx` (vollständig, 12 Zeilen)

**Interfaces:**
- Consumes: `Language`, `Translations` aus `types.ts`; `de`, `en`, `es`, `fr` aus den Wörterbüchern (Task 1+2)
- Produces:
  - `function LanguageProvider({ children }: { children: ReactNode }): JSX.Element`
  - `function useLanguage(): { language: Language; setLanguage: (next: Language) => void }`
  - `function useT(): <K extends keyof Translations>(key: K) => Translations[K]`

- [ ] **Step 1: `src/i18n/LanguageContext.tsx` schreiben**

```tsx
import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';
import type { Language, Translations } from './types';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';

const STORAGE_KEY = '3mf-katalog-language';

const DICTIONARIES: Record<Language, Translations> = { de, en, es, fr };

interface LanguageContextValue {
  language: Language;
  setLanguage: (next: Language) => void;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<Language>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored && stored in DICTIONARIES ? (stored as Language) : 'de';
  });

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  const setLanguage = useCallback((next: Language) => {
    setLanguageState(next);
    localStorage.setItem(STORAGE_KEY, next);
  }, []);

  return (
    <LanguageContext.Provider value={{ language, setLanguage }}>
      {children}
    </LanguageContext.Provider>
  );
}

function useLanguageContext(): LanguageContextValue {
  const ctx = useContext(LanguageContext);
  if (!ctx) throw new Error('useLanguage/useT must be used within a LanguageProvider');
  return ctx;
}

export function useLanguage(): LanguageContextValue {
  return useLanguageContext();
}

export function useT() {
  const { language } = useLanguageContext();
  const dict = DICTIONARIES[language];
  return useCallback(<K extends keyof Translations>(key: K): Translations[K] => dict[key], [dict]);
}
```

- [ ] **Step 2: Kompilieren prüfen (noch ohne main.tsx-Einbindung)**

Run: `npx tsc --noEmit`
Expected: PASS (Datei wird noch nirgends importiert, muss aber isoliert fehlerfrei sein)

- [ ] **Step 3: `src/main.tsx` vollständig ersetzen**

Aktueller Inhalt vor der Änderung (Referenz, exakter Wortlaut kann leicht abweichen — beim Editieren den tatsächlichen Dateiinhalt zugrunde legen):

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/theme.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

Neuer Inhalt:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LanguageProvider } from "./i18n/LanguageContext";
import "./styles/theme.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LanguageProvider>
      <App />
    </LanguageProvider>
  </React.StrictMode>,
);
```

- [ ] **Step 4: Kompilieren und Dev-Server-Start prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

Run: `npm run dev` (kurz starten, auf Vite-„ready"-Meldung warten, dann abbrechen)
Expected: Vite startet ohne Fehler; keine Runtime-Exception durch fehlenden Provider (noch nutzt keine Komponente `useLanguage`/`useT`, daher unauffällig)

- [ ] **Step 5: Commit**

```bash
git add src/i18n/LanguageContext.tsx src/main.tsx
git commit -m "$(cat <<'EOF'
feat: LanguageProvider/useLanguage/useT-Hooks und Einbindung in main.tsx

Context-basiert (nicht prop-gedrillt wie Theme), da praktisch jede
Komponente Übersetzungen braucht. localStorage-Key 3mf-katalog-language,
Default 'de', kein "System"-Modus.
EOF
)"
```

---

## Task 4: Rohdaten-Umstellung — Backend-DTO, `format.rs`-Löschung, Frontend-Typen und Wert-Rendering

**Files:**
- Modify: `src-tauri/src/lib.rs:3` (Zeile `mod format;` entfernen)
- Modify: `src-tauri/src/commands.rs:10-126` (Imports, `ModelFileDto`, `MetaRow`→`MaterialDto`, `to_dto`)
- Delete: `src-tauri/src/format.rs`
- Modify: `src/types/index.ts:5-19` (`ModelFile`-Interface)
- Modify: `src/components/DetailPanel.tsx` (gezielter Diff: Meta-Zeilen und Sync-Zeit)
- Modify: `src/components/ModelList.tsx` (gezielter Diff: Volumen-/Größen-Spalten)

**Interfaces:**
- Consumes: `useLanguage()` aus `../i18n/LanguageContext` (Task 3); `formatBytes`/`formatVolumeCm3`/`formatDimensions`/`formatDate`/`formatRelativeTime` aus `../i18n/format` (Task 1)
- Produces (Rust): `pub struct ModelFileDto { id, name, path, folder_id, tags, origin, sync, dimensions_mm: Option<[f64;3]>, volume_cm3: Option<f64>, object_count: Option<i64>, materials: Vec<MaterialDto>, file_size_bytes: i64, imported_at: String }`, `pub struct MaterialDto { name: String, display_color: Option<String> }`
- Produces (TS): restrukturiertes `ModelFile`-Interface; lokale `buildMetaRows(model: ModelFile, language: Language): { label: string; value: string }[]` in `DetailPanel.tsx` (Labels an dieser Stelle noch hartcodiert Deutsch, Werte via `format.ts` — String-Migration der Labels folgt in Task 9)

**Backend-Teil:**

- [ ] **Step 1: `src-tauri/src/lib.rs` — `mod format;` entfernen**

Alter Inhalt (Zeilen 1-8):
```rust
mod commands;
mod db;
mod format;
mod geometry;
mod stl;
mod tagging;
mod threemf;

use std::sync::Mutex;
```

Neuer Inhalt:
```rust
mod commands;
mod db;
mod geometry;
mod stl;
mod tagging;
mod threemf;

use std::sync::Mutex;
```

- [ ] **Step 2: `src-tauri/src/commands.rs` — Import entfernen**

Alter Inhalt (Zeilen 10-14):
```rust
use crate::db::models::{FileType, MaterialRecord, NewFile};
use crate::db::{self, models::FileRecord};
use crate::format;
use crate::tagging::{self, TaggingContext};
use crate::{stl, threemf};
```

Neuer Inhalt:
```rust
use crate::db::models::{FileType, MaterialRecord, NewFile};
use crate::db::{self, models::FileRecord};
use crate::tagging::{self, TaggingContext};
use crate::{stl, threemf};
```

- [ ] **Step 3: `src-tauri/src/commands.rs` — `ModelFileDto`/`MetaRow` durch `ModelFileDto`/`MaterialDto` ersetzen**

Alter Inhalt (Zeilen 22-44):
```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
    pub tags: Vec<String>,
    pub origin: String,
    pub sync: String,
    pub sync_time_label: String,
    pub volume_label: String,
    pub filesize_label: String,
    pub file_size_bytes: i64,
    pub imported_at: String,
    pub meta: Vec<MetaRow>,
}

#[derive(Debug, Serialize)]
pub struct MetaRow {
    pub label: String,
    pub value: String,
}
```

Neuer Inhalt:
```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
    pub tags: Vec<String>,
    pub origin: String,
    pub sync: String,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub materials: Vec<MaterialDto>,
    pub file_size_bytes: i64,
    pub imported_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDto {
    pub name: String,
    pub display_color: Option<String>,
}
```

- [ ] **Step 4: `src-tauri/src/commands.rs` — `to_dto` neu implementieren**

Alter Inhalt (Zeilen 68-126):
```rust
fn to_dto(file: FileRecord) -> ModelFileDto {
    let materials_label = if file.materials.is_empty() {
        "–".to_string()
    } else {
        file.materials
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let object_count_label = file
        .object_count
        .map(|c| c.to_string())
        .unwrap_or_else(|| "–".to_string());

    ModelFileDto {
        id: file.id.to_string(),
        name: file.name,
        path: file.path,
        folder_id: file
            .folder_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        tags: file.tags,
        origin: file.origin,
        sync: file.sync_status,
        sync_time_label: format::format_relative_time_de(&file.imported_at),
        volume_label: format::format_volume_cm3(file.volume_cm3),
        filesize_label: format::format_bytes(file.file_size_bytes),
        file_size_bytes: file.file_size_bytes,
        imported_at: file.imported_at.clone(),
        meta: vec![
            MetaRow {
                label: "Größe".to_string(),
                value: format::format_dimensions(file.dimensions_mm),
            },
            MetaRow {
                label: "Volumen".to_string(),
                value: format::format_volume_cm3(file.volume_cm3),
            },
            MetaRow {
                label: "Objekte".to_string(),
                value: object_count_label,
            },
            MetaRow {
                label: "Material".to_string(),
                value: materials_label,
            },
            MetaRow {
                label: "Dateigröße".to_string(),
                value: format::format_bytes(file.file_size_bytes),
            },
            MetaRow {
                label: "Importiert".to_string(),
                value: format::format_date_de(&file.imported_at),
            },
        ],
    }
}
```

Neuer Inhalt:
```rust
fn to_dto(file: FileRecord) -> ModelFileDto {
    ModelFileDto {
        id: file.id.to_string(),
        name: file.name,
        path: file.path,
        folder_id: file
            .folder_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        tags: file.tags,
        origin: file.origin,
        sync: file.sync_status,
        dimensions_mm: file.dimensions_mm,
        volume_cm3: file.volume_cm3,
        object_count: file.object_count,
        materials: file
            .materials
            .into_iter()
            .map(|m| MaterialDto {
                name: m.name,
                display_color: m.display_color,
            })
            .collect(),
        file_size_bytes: file.file_size_bytes,
        imported_at: file.imported_at,
    }
}
```

- [ ] **Step 5: `src-tauri/src/format.rs` löschen**

```bash
rm /home/thebexxs/Projekte/3mf-katalog-manager/src-tauri/src/format.rs
```

- [ ] **Step 6: Backend bauen und testen**

Run: `cd /home/thebexxs/Projekte/3mf-katalog-manager/src-tauri && cargo build`
Expected: PASS (kompiliert ohne `format`-Modul, `MaterialRecord` bleibt importiert und genutzt)

Run: `cargo test`
Expected: PASS mit 25 Tests (30 vorher − 5 aus `format.rs`)

- [ ] **Step 7: Backend-Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands.rs
git rm src-tauri/src/format.rs
git commit -m "$(cat <<'EOF'
refactor: ModelFileDto liefert rohe Felder statt deutscher Labels

format.rs vollständig entfernt — Formatierung läuft ab jetzt locale-
abhängig im Frontend über src/i18n/format.ts. MetaRow durch MaterialDto
ersetzt.
EOF
)"
```

**Frontend-Teil:**

- [ ] **Step 8: `src/types/index.ts` — `ModelFile`-Interface ersetzen**

Alter Inhalt (Zeilen 5-19):
```typescript
export interface ModelFile {
  id: string;
  name: string;
  path: string;
  folderId: string;
  tags: string[];
  origin: Origin;
  sync: SyncStatus;
  syncTimeLabel: string;
  volumeLabel: string;
  filesizeLabel: string;
  fileSizeBytes: number;
  importedAt: string;
  meta: { label: string; value: string }[];
}
```

Neuer Inhalt:
```typescript
export interface ModelFile {
  id: string;
  name: string;
  path: string;
  folderId: string;
  tags: string[];
  origin: Origin;
  sync: SyncStatus;
  dimensionsMm: [number, number, number] | null;
  volumeCm3: number | null;
  objectCount: number | null;
  materials: { name: string; displayColor: string | null }[];
  fileSizeBytes: number;
  importedAt: string;
}
```

- [ ] **Step 9: `src/components/ModelList.tsx` — Volumen-/Größen-Spalten auf `format.ts` umstellen**

Alter Inhalt (Zeile 1 sowie 59-60):
```typescript
import type { ModelFile } from '../types';
```
```tsx
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">{m.volumeLabel}</span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">{m.filesizeLabel}</span>
```

Neuer Inhalt:
```typescript
import type { ModelFile } from '../types';
import { useLanguage } from '../i18n/LanguageContext';
import { formatBytes, formatVolumeCm3 } from '../i18n/format';
```
```tsx
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">
            {formatVolumeCm3(m.volumeCm3, language)}
          </span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">
            {formatBytes(m.fileSizeBytes, language)}
          </span>
```

Innerhalb der Komponentenfunktion (direkt nach der Prop-Destrukturierung) hinzufügen:
```tsx
export function ModelList({ models, selectedId, onSelect, onContextMenu }: Props) {
  const { language } = useLanguage();
  return (
```

- [ ] **Step 10: `src/components/DetailPanel.tsx` — `buildMetaRows`-Helper einführen, Sync-Zeit umstellen**

Alter Inhalt (Zeilen 1-18):
```tsx
import { useEffect, useState } from 'react';
import type { ModelFile } from '../types';
import { ModelViewer } from './ModelViewer';

interface Props {
  model: ModelFile | null;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onOpenInSlicer: () => void;
}

const syncLabel: Record<string, string> = {
  synced: 'Aktuell',
  outdated: 'Veraltet',
  'local-only': 'Nur lokal',
  'cloud-only': 'Nur Cloud',
};
```

Neuer Inhalt:
```tsx
import { useEffect, useState } from 'react';
import type { ModelFile } from '../types';
import type { Language } from '../i18n/types';
import { useLanguage } from '../i18n/LanguageContext';
import { formatBytes, formatDate, formatDimensions, formatRelativeTime, formatVolumeCm3 } from '../i18n/format';
import { ModelViewer } from './ModelViewer';

interface Props {
  model: ModelFile | null;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onOpenInSlicer: () => void;
}

const syncLabel: Record<string, string> = {
  synced: 'Aktuell',
  outdated: 'Veraltet',
  'local-only': 'Nur lokal',
  'cloud-only': 'Nur Cloud',
};

function buildMetaRows(model: ModelFile, language: Language): { label: string; value: string }[] {
  const materialsValue =
    model.materials.length === 0
      ? '–'
      : model.materials.map((m) => m.name).join(', ');
  const objectCountValue = model.objectCount === null ? '–' : String(model.objectCount);

  return [
    { label: 'Größe', value: formatDimensions(model.dimensionsMm, language) },
    { label: 'Volumen', value: formatVolumeCm3(model.volumeCm3, language) },
    { label: 'Objekte', value: objectCountValue },
    { label: 'Material', value: materialsValue },
    { label: 'Dateigröße', value: formatBytes(model.fileSizeBytes, language) },
    { label: 'Importiert', value: formatDate(model.importedAt, language) },
  ];
}
```

Alter Inhalt (Zeile 20-21 sowie 68-69, 76-86):
```tsx
export function DetailPanel({ model, onAddTag, onRemoveTag, onDelete, onOpenInSlicer }: Props) {
  const [draft, setDraft] = useState('');
```
```tsx
          <span className="flex-1 text-[12.5px] font-medium">{syncLabel[model.sync]}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">{model.syncTimeLabel}</span>
```
```tsx
          {model.meta.map((row) => (
```

Neuer Inhalt:
```tsx
export function DetailPanel({ model, onAddTag, onRemoveTag, onDelete, onOpenInSlicer }: Props) {
  const { language } = useLanguage();
  const [draft, setDraft] = useState('');
```
```tsx
          <span className="flex-1 text-[12.5px] font-medium">{syncLabel[model.sync]}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
            {formatRelativeTime(model.importedAt, language)}
          </span>
```
```tsx
          {buildMetaRows(model, language).map((row) => (
```

- [ ] **Step 11: Frontend kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 12: Manuell im Browser prüfen (Dev-Server)**

Run: `npm run tauri dev` kurz starten
Erwartet: Modell-Liste und Detail-Panel zeigen weiterhin plausible Werte für Volumen, Dateigröße, Datum, Dimensionen — keine `undefined`/`NaN`-Ausgaben. Danach Dev-Server beenden.

- [ ] **Step 13: Frontend-Commit**

```bash
git add src/types/index.ts src/components/ModelList.tsx src/components/DetailPanel.tsx
git commit -m "$(cat <<'EOF'
refactor: Frontend rendert rohe Modelldaten über Intl-Formatierung

ModelFile-Interface an die neuen Backend-Rohfelder angepasst.
buildMetaRows() in DetailPanel.tsx ersetzt das serverseitige meta-Array;
Labels bleiben an dieser Stelle noch Deutsch (String-Migration folgt).
EOF
)"
```

---

## Task 5: `Header.tsx` — Sprache-Umschalter und vollständige String-Migration

**Files:**
- Modify: `src/components/Header.tsx` (vollständige Datei, 163 Zeilen)

**Interfaces:**
- Consumes: `useT()`, `useLanguage()` aus `../i18n/LanguageContext`; `formatCount` aus `../i18n/types`; `Language` aus `../i18n/types`
- Produces: keine neuen Exporte — reiner UI-String- und Sprachschalter-Umbau

- [ ] **Step 1: `src/components/Header.tsx` vollständig ersetzen**

```tsx
import { useState } from 'react';
import type { ViewMode, SortKey } from '../types';
import type { ThemeSetting } from '../hooks/useTheme';
import type { Language } from '../i18n/types';
import { formatCount } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';

interface Props {
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortKey;
  onSortChange: (s: SortKey) => void;
  count: number;
  themeSetting: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  onImportFiles: () => void;
  onImportFolder: () => void;
}

const segBase =
  'h-[26px] px-3 rounded-[2px] text-[12.5px] font-medium cursor-pointer transition-colors';
const segActive = 'bg-[var(--accent)] text-[var(--accent-ink)]';
const segInactive = 'text-[var(--ink-2)] hover:text-[var(--ink)]';

const LANGUAGE_LABELS: Record<Language, string> = {
  de: 'Deutsch',
  en: 'English',
  es: 'Español',
  fr: 'Français',
};

export function Header({
  view,
  onViewChange,
  sort,
  onSortChange,
  count,
  themeSetting,
  onThemeChange,
  onImportFiles,
  onImportFolder,
}: Props) {
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [importMenuOpen, setImportMenuOpen] = useState(false);

  return (
    <header className="flex-none h-[54px] flex items-center gap-[18px] px-[14px] bg-[var(--panel)] border-b border-[var(--line)]">
      <div className="flex items-baseline gap-2 pr-1.5">
        <span className="text-[15px] font-bold tracking-[0.06em] uppercase">
          3MF Katalog
        </span>
        <span className="font-mono-ui text-[11px] text-[var(--accent)] tracking-[0.08em]">
          MANAGER
        </span>
      </div>

      <div className="relative flex">
        <button
          onClick={() => {
            setImportMenuOpen(false);
            onImportFiles();
          }}
          className="flex items-center gap-2 h-8 pl-[13px] pr-3 rounded-l-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[13px] font-semibold cursor-pointer hover:brightness-110"
        >
          <span className="font-mono-ui text-sm leading-none">+</span>
          <span>{t('import')}</span>
        </button>
        <button
          onClick={() => setImportMenuOpen((o) => !o)}
          aria-label={t('importMoreOptionsAria')}
          className="flex items-center justify-center w-6 h-8 rounded-r-[3px] border border-l-0 border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] cursor-pointer hover:brightness-110"
        >
          <span className="text-[9px] leading-none">▾</span>
        </button>

        {importMenuOpen && (
          <div className="absolute top-10 left-0 w-[176px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFiles();
              }}
              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFilesOption')}
            </button>
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolder();
              }}
              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderOption')}
            </button>
          </div>
        )}
      </div>

      <div className="flex items-center gap-1.5">
        <span className="font-mono-ui text-[10px] tracking-[0.1em] uppercase text-[var(--ink-3)]">
          {t('sortLabel')}
        </span>
        <select
          value={sort}
          onChange={(e) => onSortChange(e.target.value as SortKey)}
          className="h-[30px] px-2 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink)] text-[13px] cursor-pointer"
        >
          <option value="name">{t('sortName')}</option>
          <option value="date">{t('sortDate')}</option>
          <option value="size">{t('sortSize')}</option>
          <option value="vol">{t('sortVolume')}</option>
        </select>
      </div>

      <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
        <button
          onClick={() => onViewChange('grid')}
          className={`${segBase} ${view === 'grid' ? segActive : segInactive}`}
        >
          {t('viewGrid')}
        </button>
        <button
          onClick={() => onViewChange('list')}
          className={`${segBase} ${view === 'list' ? segActive : segInactive}`}
        >
          {t('viewList')}
        </button>
      </div>

      <div className="flex-1" />

      <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">
        {formatCount(t('filesCount'), count)}
      </span>

      <div className="relative">
        <button
          onClick={() => setSettingsOpen((o) => !o)}
          className="w-8 h-8 grid place-items-center rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-[15px] cursor-pointer hover:text-[var(--ink)] hover:border-[var(--line-strong)]"
        >
          ⚙
        </button>

        {settingsOpen && (
          <div className="absolute top-10 right-0 w-[268px] p-[14px] bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
            <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] mb-2.5">
              {t('settingsTitle')}
            </div>
            <div className="text-[13px] font-semibold mb-2">{t('appearanceTitle')}</div>
            <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
              {(['system', 'light', 'dark'] as ThemeSetting[]).map((opt) => (
                <button
                  key={opt}
                  onClick={() => onThemeChange(opt)}
                  className={`${segBase} flex-1 ${themeSetting === opt ? segActive : segInactive}`}
                >
                  {opt === 'system' ? t('themeSystem') : opt === 'light' ? t('themeLight') : t('themeDark')}
                </button>
              ))}
            </div>
            <div className="mt-2 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">
              {themeSetting === 'system'
                ? t('themeDescriptionSystem')
                : t('themeDescriptionManual').replace(
                    '{mode}',
                    themeSetting === 'light' ? t('themeLight') : t('themeDark'),
                  )}
            </div>

            <div className="text-[13px] font-semibold mt-4 mb-2">{t('languageTitle')}</div>
            <div className="grid grid-cols-2 gap-0.5 p-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
              {(['de', 'en', 'es', 'fr'] as Language[]).map((lang) => (
                <button
                  key={lang}
                  onClick={() => setLanguage(lang)}
                  className={`${segBase} ${language === lang ? segActive : segInactive}`}
                >
                  {LANGUAGE_LABELS[lang]}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </header>
  );
}
```

- [ ] **Step 2: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Manuell im Browser prüfen**

Run: `npm run tauri dev`
Erwartet: Einstellungen-Panel zeigt neuen „Sprache"-Abschnitt als 2×2-Raster unter „Erscheinungsbild"; Klick auf „English"/„Español"/„Français" schaltet sofort `import`/`sortLabel`/`viewGrid`/`viewList`/Dateizähler auf die jeweilige Sprache um (übrige Komponenten bleiben vorerst Deutsch — das ist bis Task 9/10/11 erwartet); Reload behält die gewählte Sprache (localStorage). Danach Dev-Server beenden.

- [ ] **Step 4: Commit**

```bash
git add src/components/Header.tsx
git commit -m "$(cat <<'EOF'
feat: Sprache-Umschalter in Header.tsx, String-Migration Header

2×2-Button-Raster im Einstellungen-Panel, wiederverwendet bestehende
seg*-Klassen. Sprachnamen bleiben im Original (Deutsch/English/Español/
Français), unabhängig von der aktiven UI-Sprache.
EOF
)"
```

---

## Task 6: `Sidebar.tsx` — String-Migration

**Files:**
- Modify: `src/components/Sidebar.tsx` (vollständige Datei, 145 Zeilen)

**Interfaces:**
- Consumes: `useT()` aus `../i18n/LanguageContext`

- [ ] **Step 1: `src/components/Sidebar.tsx` vollständig ersetzen**

```tsx
import type { Folder, TagCount, CloudAccount } from '../types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  folders: Folder[];
  activeFolderId: string;
  onFolderSelect: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  onTagSelect: (label: string | null) => void;
  clouds: CloudAccount[];
  onAddCloud: () => void;
}

const originAbbr: Record<string, string> = {
  gdrive: 'GD',
  onedrive: 'OD',
  dropbox: 'DB',
  proton: 'PD',
};

export function Sidebar({
  query,
  onQueryChange,
  folders,
  activeFolderId,
  onFolderSelect,
  tags,
  activeTag,
  onTagSelect,
  clouds,
  onAddCloud,
}: Props) {
  const t = useT();

  return (
    <aside className="flex-none w-[242px] flex flex-col min-h-0 bg-[var(--panel)] border-r border-[var(--line)]">
      <div className="p-3 pb-2.5 border-b border-[var(--line)]">
        <div className="flex items-center gap-1.5 h-8 px-2.5 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)]">
          <span className="font-mono-ui text-xs text-[var(--ink-3)]">⌕</span>
          <input
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder={t('searchPlaceholder')}
            className="flex-1 min-w-0 border-0 outline-0 bg-transparent text-[var(--ink)] text-[13px]"
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2 py-3">
        <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] px-1.5 pb-2">
          {t('foldersHeading')}
        </div>
        {folders.map((f) => (
          <div
            key={f.id}
            onClick={() => onFolderSelect(f.id)}
            className={`flex items-center gap-2 h-8 px-1.5 rounded-[3px] text-[13px] cursor-pointer ${
              f.id === activeFolderId
                ? 'bg-[var(--panel-2)] text-[var(--ink)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
              {f.name}
            </span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{f.count}</span>
          </div>
        ))}

        <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] px-1.5 pt-[18px] pb-2">
          {t('tagsHeading')}
        </div>
        {tags.map((tag) => (
          <div
            key={tag.label}
            onClick={() => onTagSelect(activeTag === tag.label ? null : tag.label)}
            className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer ${
              activeTag === tag.label
                ? 'bg-[var(--accent-soft)] text-[var(--accent)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span
              className="w-[7px] h-[7px] rounded-full"
              style={{ background: `oklch(0.62 0.14 ${tag.colorHue})` }}
            />
            <span className="flex-1 font-mono-ui text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              #{tag.label}
            </span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{tag.count}</span>
          </div>
        ))}
      </div>

      <div className="flex-none border-t border-[var(--line)] px-3.5 pt-3 pb-3.5">
        <div className="flex items-center justify-between pb-2.5">
          <span className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('cloudAccountsHeading')}
          </span>
          <span
            onClick={onAddCloud}
            className="font-mono-ui text-sm leading-none text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
          >
            +
          </span>
        </div>
        {clouds.map((c) => (
          <div key={c.id} className="flex flex-col gap-1.5 py-1.5">
            <div className="flex items-center gap-2">
              <span className="font-mono-ui text-[10px] px-1 py-0.5 rounded border border-[var(--line-strong)] text-[var(--ink-2)]">
                {originAbbr[c.id] ?? c.abbr}
              </span>
              <span className="flex-1 text-[12.5px] font-medium overflow-hidden text-ellipsis whitespace-nowrap">
                {c.name}
              </span>
              <span
                className={`font-mono-ui text-[10px] ${
                  c.status === 'connected' ? 'text-[var(--ink-3)]' : 'text-[var(--accent)]'
                }`}
              >
                {c.status === 'connected'
                  ? t('cloudConnected')
                  : c.status === 'error'
                  ? t('cloudError')
                  : t('cloudDisconnected')}
              </span>
            </div>
            <div className="flex items-center gap-2 pl-[26px]">
              <div className="flex-1 h-[3px] rounded bg-[var(--line)] overflow-hidden">
                <div
                  className="h-full bg-[var(--accent)]"
                  style={{ width: `${c.usedPercent}%` }}
                />
              </div>
              <span className="font-mono-ui text-[10px] text-[var(--ink-3)] whitespace-nowrap">
                {c.quotaLabel}
              </span>
            </div>
          </div>
        ))}
      </div>
    </aside>
  );
}
```

- [ ] **Step 2: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/components/Sidebar.tsx
git commit -m "$(cat <<'EOF'
feat: String-Migration Sidebar.tsx

Suchfeld-Platzhalter, Ordner-/Tags-/Cloud-Konten-Überschriften und
Cloud-Status-Labels übersetzt. tags.map-Schleifenvariable von t auf tag
umbenannt (Namenskollision mit useT()).
EOF
)"
```

---

## Task 7: `ModelGrid.tsx` — String-Migration

**Files:**
- Modify: `src/components/ModelGrid.tsx` (vollständige Datei, 74 Zeilen)

**Interfaces:**
- Consumes: `useT()` aus `../i18n/LanguageContext`

- [ ] **Step 1: `src/components/ModelGrid.tsx` vollständig ersetzen**

```tsx
import type { ModelFile } from '../types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  models: ModelFile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
}

const originAbbr: Record<string, string> = {
  local: '',
  gdrive: 'GD',
  onedrive: 'OD',
  dropbox: 'DB',
  proton: 'PD',
};

export function ModelGrid({ models, selectedId, onSelect, onContextMenu }: Props) {
  const t = useT();

  return (
    <div className="grid gap-3.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(178px, 1fr))' }}>
      {models.map((m) => (
        <div
          key={m.id}
          onClick={() => onSelect(m.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            onSelect(m.id);
            onContextMenu(m.id, e.clientX, e.clientY);
          }}
          className={`rounded-[4px] overflow-hidden border cursor-pointer ${
            m.id === selectedId ? 'border-[var(--accent)]' : 'border-[var(--line)]'
          }`}
        >
          <div className="relative aspect-square bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
            <div
              className="absolute inset-0 opacity-90"
              style={{
                backgroundImage:
                  'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 9px)',
              }}
            />
            <div className="absolute inset-0 grid place-items-center">
              <div className="w-[52px] h-[52px] border border-dashed border-[var(--line-strong)] rotate-45" />
            </div>
            <div className="absolute left-2 bottom-[7px] font-mono-ui text-[9px] tracking-[0.08em] uppercase text-[var(--ink-3)]">
              {t('previewLabel3d')}
            </div>
            {originAbbr[m.origin] && (
              <div className="absolute right-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
                {originAbbr[m.origin]}
              </div>
            )}
          </div>
          <div className="flex flex-col gap-1.5 px-2.5 py-2.5 bg-[var(--panel)]">
            <div className="text-[12.5px] font-semibold overflow-hidden text-ellipsis whitespace-nowrap">
              {m.name}
            </div>
            <div className="flex flex-wrap gap-1">
              {m.tags.map((tag) => (
                <span
                  key={tag}
                  className="font-mono-ui text-[10px] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]"
                >
                  #{tag}
                </span>
              ))}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/components/ModelGrid.tsx
git commit -m "$(cat <<'EOF'
feat: String-Migration ModelGrid.tsx

"3D Vorschau"-Overlay-Text übersetzt. tags.map-Schleifenvariable von t
auf tag umbenannt (Namenskollision mit useT()).
EOF
)"
```

---

## Task 8: `ModelList.tsx` — String-Migration (Spaltenüberschriften, Sync-Labels)

**Files:**
- Modify: `src/components/ModelList.tsx` (vollständige Datei — baut auf dem Task-4-Zwischenstand auf)

**Interfaces:**
- Consumes: `useT()` aus `../i18n/LanguageContext` (zusätzlich zu `useLanguage()` aus Task 4)

- [ ] **Step 1: `src/components/ModelList.tsx` vollständig ersetzen**

```tsx
import type { ModelFile } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatVolumeCm3 } from '../i18n/format';
import type { Translations } from '../i18n/types';

interface Props {
  models: ModelFile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
}

const SYNC_KEYS: Record<string, keyof Translations> = {
  synced: 'syncSynced',
  outdated: 'syncOutdated',
  'local-only': 'syncLocalOnly',
  'cloud-only': 'syncCloudOnly',
};

export function ModelList({ models, selectedId, onSelect, onContextMenu }: Props) {
  const { language } = useLanguage();
  const t = useT();

  return (
    <div className="border border-[var(--line)] rounded overflow-x-auto bg-[var(--panel)]">
      <div
        className="min-w-[680px] grid gap-2.5 items-center px-3 py-2 bg-[var(--panel-2)] border-b border-[var(--line)] font-mono-ui text-[10px] tracking-[0.1em] uppercase text-[var(--ink-3)]"
        style={{ gridTemplateColumns: '62px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px 74px' }}
      >
        <span>{t('columnOrigin')}</span>
        <span>{t('columnName')}</span>
        <span>{t('columnTags')}</span>
        <span>{t('columnVolume')}</span>
        <span>{t('columnSize')}</span>
        <span>{t('columnSync')}</span>
      </div>
      {models.map((m) => (
        <div
          key={m.id}
          onClick={() => onSelect(m.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            onSelect(m.id);
            onContextMenu(m.id, e.clientX, e.clientY);
          }}
          className={`min-w-[680px] grid gap-2.5 items-center px-3 py-2 border-b border-[var(--line)] cursor-pointer ${
            m.id === selectedId ? 'bg-[var(--accent-soft)]' : 'hover:bg-[var(--panel-2)]'
          }`}
          style={{ gridTemplateColumns: '62px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px 74px' }}
        >
          <span className="font-mono-ui text-[10px] text-[var(--ink-3)]">{m.origin}</span>
          <span className="text-[13px] font-medium overflow-hidden text-ellipsis whitespace-nowrap">
            {m.name}
          </span>
          <span className="flex gap-1 overflow-hidden">
            {m.tags.map((tag) => (
              <span
                key={tag}
                className="font-mono-ui text-[10px] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap"
              >
                #{tag}
              </span>
            ))}
          </span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">
            {formatVolumeCm3(m.volumeCm3, language)}
          </span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">
            {formatBytes(m.fileSizeBytes, language)}
          </span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-2)]">
            {t(SYNC_KEYS[m.sync])}
          </span>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/components/ModelList.tsx
git commit -m "$(cat <<'EOF'
feat: String-Migration ModelList.tsx

Spaltenüberschriften und Sync-Status-Labels übersetzt; Sync-Labels
nutzen jetzt den mit DetailPanel.tsx geteilten Key-Satz (syncSynced/
syncOutdated/syncLocalOnly/syncCloudOnly). tags.map-Schleifenvariable
von t auf tag umbenannt (Namenskollision mit useT()).
EOF
)"
```

---

## Task 9: `DetailPanel.tsx` — String-Migration (Meta-Labels, Buttons, Leerzustand, Löschbestätigung)

**Files:**
- Modify: `src/components/DetailPanel.tsx` (vollständige Datei — baut auf dem Task-4-Zwischenstand auf)

**Interfaces:**
- Consumes: `useT()` aus `../i18n/LanguageContext` (zusätzlich zu `useLanguage()` aus Task 4)
- Produces: `buildMetaRows`-Signatur ändert sich zu `(model: ModelFile, t: TFunction, language: Language)` — dies ist die im Design vorgesehene zweite und letzte Änderung dieser Funktion

- [ ] **Step 1: `src/components/DetailPanel.tsx` vollständig ersetzen**

```tsx
import { useEffect, useState } from 'react';
import type { ModelFile } from '../types';
import type { Language, Translations } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatDate, formatDimensions, formatRelativeTime, formatVolumeCm3 } from '../i18n/format';
import { ModelViewer } from './ModelViewer';

interface Props {
  model: ModelFile | null;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onOpenInSlicer: () => void;
}

type TFunction = <K extends keyof Translations>(key: K) => Translations[K];

const SYNC_KEYS: Record<string, keyof Translations> = {
  synced: 'syncSynced',
  outdated: 'syncOutdated',
  'local-only': 'syncLocalOnly',
  'cloud-only': 'syncCloudOnly',
};

function buildMetaRows(model: ModelFile, t: TFunction, language: Language): { label: string; value: string }[] {
  const materialsValue =
    model.materials.length === 0
      ? t('noValue')
      : model.materials.map((m) => m.name).join(', ');
  const objectCountValue = model.objectCount === null ? t('noValue') : String(model.objectCount);

  return [
    { label: t('metaDimensions'), value: formatDimensions(model.dimensionsMm, language) },
    { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    { label: t('metaObjectCount'), value: objectCountValue },
    { label: t('metaMaterial'), value: materialsValue },
    { label: t('metaFileSize'), value: formatBytes(model.fileSizeBytes, language) },
    { label: t('metaImported'), value: formatDate(model.importedAt, language) },
  ];
}

export function DetailPanel({ model, onAddTag, onRemoveTag, onDelete, onOpenInSlicer }: Props) {
  const { language } = useLanguage();
  const t = useT();
  const [draft, setDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => setConfirmDelete(false), [model?.id]);

  if (!model) {
    return (
      <aside className="flex-none w-[336px] flex items-center justify-center bg-[var(--panel)] border-l border-[var(--line)] text-[var(--ink-3)] text-[13px] px-6 text-center">
        {t('emptyStateText')}
      </aside>
    );
  }

  const submitDraft = () => {
    const value = draft.trim().replace(/^#/, '');
    if (value) onAddTag(value);
    setDraft('');
  };

  return (
    <aside className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
      <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
        <div className="text-[14.5px] font-semibold leading-tight break-words">{model.name}</div>
        <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)] pt-1.5">{model.path}</div>
      </div>

      <div className="flex-1 overflow-y-auto">
        <div className="relative aspect-[4/3] bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 11px)',
            }}
          />
          <ModelViewer key={model.id} fileId={model.id} />
          <div className="absolute left-2.5 bottom-2 font-mono-ui text-[9.5px] tracking-[0.08em] uppercase text-[var(--ink-3)] pointer-events-none">
            {t('dragToRotate')}
          </div>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span
            className={`w-2 h-2 rounded-full ${
              model.sync === 'synced' ? 'bg-[var(--accent)]' : 'bg-[var(--ink-3)]'
            }`}
          />
          <span className="flex-1 text-[12.5px] font-medium">{t(SYNC_KEYS[model.sync])}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
            {formatRelativeTime(model.importedAt, language)}
          </span>
        </div>

        <div className="px-4 pt-3.5 pb-1">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
            {t('metadataHeading')}
          </div>
          {buildMetaRows(model, t, language).map((row) => (
            <div
              key={row.label}
              className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]"
            >
              <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
                {row.label}
              </span>
              <span className="flex-1 font-mono-ui text-xs text-right">{row.value}</span>
            </div>
          ))}
        </div>

        <div className="px-4 pt-[18px] pb-5">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2.5">
            {t('hashtagsHeading')}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {model.tags.map((tag) => (
              <span
                key={tag}
                className="inline-flex items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11.5px]"
              >
                #{tag}
                <span
                  onClick={() => onRemoveTag(tag)}
                  className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                >
                  ✕
                </span>
              </span>
            ))}
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submitDraft()}
              placeholder={t('addTagPlaceholder')}
              className="h-6 w-[118px] px-2.5 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-[11.5px]"
            />
          </div>
        </div>
      </div>

      <div className="flex-none flex gap-2 px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        {confirmDelete ? (
          <>
            <span className="flex-1 flex items-center text-[12.5px] font-medium text-[var(--ink)]">
              {t('deleteConfirmQuestion')}
            </span>
            <button
              onClick={() => setConfirmDelete(false)}
              className="flex-none h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
            <button
              onClick={() => {
                setConfirmDelete(false);
                onDelete();
              }}
              className="flex-none h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
            >
              {t('delete')}
            </button>
          </>
        ) : (
          <>
            <button
              onClick={onOpenInSlicer}
              className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('openInSlicer')}
            </button>
            <button className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-mono-ui cursor-pointer">
              ↻
            </button>
            <button
              onClick={() => setConfirmDelete(true)}
              aria-label={t('deleteAriaLabel')}
              className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-mono-ui cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              ✕
            </button>
          </>
        )}
      </div>
    </aside>
  );
}
```

- [ ] **Step 2: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/components/DetailPanel.tsx
git commit -m "$(cat <<'EOF'
feat: String-Migration DetailPanel.tsx

Leerzustand, Viewer-Hinweis, Metadaten-Überschrift/-Zeilenlabels,
Hashtags-Bereich, Löschbestätigung und Buttons übersetzt. buildMetaRows
nimmt jetzt t() entgegen und übersetzt Zeilenlabels. Sync-Labels teilen
sich den Key-Satz mit ModelList.tsx. Tag-Schleifenvariable von t auf tag,
lokale Variable im Tag-Submit-Handler von t auf value umbenannt
(Namenskollisionen mit useT()).
EOF
)"
```

---

## Task 10: `ContextMenu.tsx` — String-Migration (geteilte Keys mit `DetailPanel.tsx`)

**Files:**
- Modify: `src/components/ContextMenu.tsx` (vollständige Datei, 79 Zeilen)

**Interfaces:**
- Consumes: `useT()` aus `../i18n/LanguageContext`; nutzt dieselben Keys wie `DetailPanel.tsx` (`deleteConfirmQuestion`, `cancel`, `delete`, `openInSlicer`)

- [ ] **Step 1: `src/components/ContextMenu.tsx` vollständig ersetzen**

```tsx
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';

interface Props {
  x: number;
  y: number;
  onClose: () => void;
  onOpenInSlicer: () => void;
  onDelete: () => void;
}

export function ContextMenu({ x, y, onClose, onOpenInSlicer, onDelete }: Props) {
  const t = useT();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handlePointerDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('mousedown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('mousedown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      className="fixed z-50 min-w-[172px] rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-lg overflow-hidden"
      style={{ left: x, top: y }}
    >
      {confirmDelete ? (
        <div className="px-3 py-2.5">
          <div className="text-[12px] font-medium text-[var(--ink)] pb-2">{t('deleteConfirmQuestion')}</div>
          <div className="flex gap-1.5">
            <button
              onClick={() => setConfirmDelete(false)}
              className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
            <button
              onClick={() => {
                onDelete();
                onClose();
              }}
              className="flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11.5px] font-semibold cursor-pointer"
            >
              {t('delete')}
            </button>
          </div>
        </div>
      ) : (
        <>
          <button
            onClick={() => {
              onOpenInSlicer();
              onClose();
            }}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('openInSlicer')}
          </button>
          <button
            onClick={() => setConfirmDelete(true)}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('delete')}
          </button>
        </>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/components/ContextMenu.tsx
git commit -m "$(cat <<'EOF'
feat: String-Migration ContextMenu.tsx

Löschbestätigung und Buttons nutzen die mit DetailPanel.tsx geteilten
Keys (deleteConfirmQuestion/cancel/delete/openInSlicer).
EOF
)"
```

---

## Task 11: `ModelViewer.tsx` — String-Migration (gezielter Diff)

**Files:**
- Modify: `src/components/ModelViewer.tsx:1, 43-45, 149-157`

**Interfaces:**
- Consumes: `useT()` aus `../i18n/LanguageContext`

- [ ] **Step 1: Import ergänzen**

Alter Inhalt (Zeile 1):
```typescript
import { useEffect, useRef, useState } from 'react';
```

Neuer Inhalt:
```typescript
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
```

- [ ] **Step 2: Hook im Komponentenkörper ergänzen**

Alter Inhalt (Zeilen 43-45):
```tsx
export function ModelViewer({ fileId }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
```

Neuer Inhalt:
```tsx
export function ModelViewer({ fileId }: Props) {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
```

- [ ] **Step 3: Status-Texte übersetzen**

Alter Inhalt (Zeilen 149-157):
```tsx
      {status === 'loading' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none">
          Lädt Vorschau …
        </div>
      )}
      {status === 'error' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none px-4 text-center">
          Vorschau nicht verfügbar
        </div>
      )}
```

Neuer Inhalt:
```tsx
      {status === 'loading' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none">
          {t('loadingPreview')}
        </div>
      )}
      {status === 'error' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none px-4 text-center">
          {t('previewUnavailable')}
        </div>
      )}
```

Hinweis: `throw new Error('nicht unterstütztes Format: ...')` (Zeile 117) und `console.error('[ModelViewer] Laden fehlgeschlagen:', ...)` (Zeile 126) bleiben unangetastet — beide sind nicht user-facing (interne Fehlerbehandlung/Konsolen-Log), außerhalb des Spec-Scopes.

- [ ] **Step 4: Kompilieren prüfen**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/components/ModelViewer.tsx
git commit -m "$(cat <<'EOF'
feat: String-Migration ModelViewer.tsx

Lade-/Fehlertext übersetzt. Interne, nicht user-facing Fehlermeldungen
(throw/console.error) bleiben unangetastet.
EOF
)"
```

---

## Task 12: Abschlussverifikation

**Files:** keine (nur Build/Test-Ausführung und manuelles Testprotokoll)

**Interfaces:** keine

- [ ] **Step 1: Backend vollständig bauen und testen**

Run: `cd /home/thebexxs/Projekte/3mf-katalog-manager/src-tauri && cargo build && cargo test`
Expected: Build PASS, 25 Tests PASS

- [ ] **Step 2: Frontend vollständig bauen**

Run: `cd /home/thebexxs/Projekte/3mf-katalog-manager && npm run build`
Expected: PASS (führt `tsc && vite build` aus — sowohl Typprüfung als auch Produktions-Build müssen fehlerfrei durchlaufen)

- [ ] **Step 3: App starten und manuellen Testdurchlauf gemäß Spec Abschnitt 4 durchführen**

Run: `npm run tauri dev`

Manuelle Prüfliste:
- [ ] Sprachumschaltung DE→EN→ES→FR→DE über das Einstellungen-Panel; jede Sprache zeigt konsistente Übersetzungen in Header, Sidebar, ModelGrid/ModelList, DetailPanel, ContextMenu, ModelViewer-Statustexten.
- [ ] Metadaten-Formatierung stichprobenartig je Sprache prüfen: Datum (`metaImported`/Sync-Zeit), Volumen (`cm³`, Dezimaltrennzeichen je Locale), Dateigröße (B/KB/MB/GB-Stufen), Dimensionen (`× ... × ... mm`).
- [ ] Pluralisierung des Dateizählers bei 0, 1 und mehreren Dateien in mindestens zwei Sprachen (z. B. Deutsch „1 Datei"/„3 Dateien", Englisch „1 file"/„3 files").
- [ ] localStorage-Persistenz: Sprache wählen, Seite neu laden (F5 bzw. App neu starten), Sprache bleibt erhalten.
- [ ] Regressionscheck: Theme-Umschaltung (Hell/Dunkel/System), Tag hinzufügen/entfernen, Löschbestätigung, Kontextmenü, 3D-Vorschau funktionieren weiterhin unverändert.

Dev-Server danach beenden.

- [ ] **Step 4: Finalen Git-Log prüfen**

Run: `git log --oneline -15`
Expected: Alle 11 Feature-/Refactor-Commits dieses Plans sind sichtbar, in der richtigen Reihenfolge, keine offenen Änderungen (`git status --short` zeigt nichts Unerwartetes).

- [ ] **Step 5: Abschluss-Zusammenfassung an den User**

Kurze Zusammenfassung: i18n-Feature vollständig implementiert (DE/EN/ES/FR), Backend liefert Rohdaten, Frontend formatiert über `Intl`, alle sieben Komponenten migriert, Build und Tests grün, manueller Testdurchlauf abgeschlossen.

---

## Self-Review (durchgeführt beim Schreiben dieses Plans)

**Spec-Abdeckung:**
- Abschnitt 1 (Architektur & Dateistruktur): Task 1–3 (types.ts, format.ts, de/en/es/fr.ts, LanguageContext.tsx, main.tsx-Wiring). ✓
- Abschnitt 2 (Backend-DTO-Umbau, `format.rs`-Löschung): Task 4, Backend-Teil (Step 1–7). ✓
- Abschnitt 3 (Frontend-Integration & String-Migration, Sprache-Umschalter, `buildMetaRows`, Pluralisierung): Task 4 Frontend-Teil + Task 5–11. ✓
- Abschnitt 4 (Testplan): Rot/Grün-Zyklen für Wörterbücher (Task 1–2), `tsc --noEmit`-Gate für jede Komponenten-Aufgabe, manuelle Browser-Prüfliste in Task 12. ✓
- Nicht-Ziele (keine System-Spracherkennung, keine Backend-Fehlerübersetzung, keine neue Test-Infrastruktur, keine i18n-Bibliothek): in Global Constraints festgehalten, an keiner Stelle verletzt. ✓

**Placeholder-Scan:** Keine „TBD"/„TODO"/„später ergänzen"-Stellen; jeder Code-Block enthält vollständigen, lauffähigen Code; jede Testverifikation nennt den exakten Befehl und das erwartete Ergebnis.

**Typkonsistenz:** `Translations`-Interface (Task 1) wird identisch in allen vier Wörterbüchern (Task 1–2), `LanguageContext.tsx` (Task 3) und allen Komponenten (Task 5–11) verwendet. `ModelFileDto`/`MaterialDto` (Rust, Task 4) und `ModelFile`/`materials` (TS, Task 4) sind feldkonsistent (`dimensionsMm`/`volumeCm3`/`objectCount`/`materials` beidseitig). `buildMetaRows` wird in Task 4 mit Signatur `(model, language)` eingeführt und in Task 9 bewusst zu `(model, t, language)` erweitert — beide Zwischenstände sind für sich kompilierbar, wie im Design vorgesehen. `SYNC_KEYS`-Mapping ist in Task 8 (`ModelList.tsx`) und Task 9 (`DetailPanel.tsx`) identisch definiert (geteilter Key-Satz `syncSynced`/`syncOutdated`/`syncLocalOnly`/`syncCloudOnly`). Schleifenvariablen-Umbenennungen (`t`→`opt`/`tag`/`value`) sind in allen betroffenen Dateien konsistent angewendet, inklusive der in der bisherigen Analyse nicht explizit genannten Kollision in `ModelList.tsx` (Task 8), die beim erneuten Lesen der Datei zusätzlich identifiziert wurde.
