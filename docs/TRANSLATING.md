# Translating

🇩🇪 **Deutsch:** [Übersetzen weiter unten](#übersetzen-deutsch)

The app UI is available in German, English, Spanish, and French. This document explains how translations are structured in the code and how to add or fix one.

## How it works

All translatable strings are defined by one TypeScript interface, `Translations`, in [`src/i18n/types.ts`](../src/i18n/types.ts). Each language then gets its own file that implements that interface:

- [`src/i18n/de.ts`](../src/i18n/de.ts) — German
- [`src/i18n/en.ts`](../src/i18n/en.ts) — English
- [`src/i18n/es.ts`](../src/i18n/es.ts) — Spanish
- [`src/i18n/fr.ts`](../src/i18n/fr.ts) — French

Because each file is typed as `Translations`, **all four files must define exactly the same set of keys.** If a key is missing or misspelled in any one file, TypeScript won't compile — running `npx tsc --noEmit` (or `npm run build`) catches this immediately. [`src/i18n/LanguageContext.tsx`](../src/i18n/LanguageContext.tsx) picks the active dictionary at runtime and looks up strings by key via the `useT()` hook.

Two things to know about the string values themselves:

- **Plural forms.** Some entries use `PluralForms` instead of a plain string: `{ one: '...', other: '...' }`. The helper `formatCount()` in `types.ts` picks `one` for a count of exactly 1 and `other` otherwise, substituting `{count}`. Example (from `en.ts`):
  ```ts
  filesCount: { one: '{count} file', other: '{count} files' },
  ```
- **Placeholders.** Some strings contain `{something}` placeholders that get substituted with a value at runtime — e.g. `{version}`, `{mode}`, `{count}`, `{spool}`. Keep the placeholder names identical to the other languages' version of the same key; only the surrounding text should change. Example (from `en.ts`):
  ```ts
  infoAppVersionLabel: 'Version {version}',
  ```

## Adding or correcting a string

- **Correcting an existing translation:** find the key in the language file you want to fix (e.g. search for the English text in `en.ts` to find the key, then edit the same key in `de.ts`/`es.ts`/`fr.ts`) and change the value. Keep any `{placeholder}` tokens intact.
- **Adding a new string** (usually alongside a new feature): add the key to the `Translations` interface in `types.ts` with a short doc comment if its usage isn't obvious (see the existing examples for `{version}`/`{count}`/`{spool}` placeholders), then add the same key with an appropriate value to **all four** language files. Missing it in one of them will fail the build.

## Checking your change

Run the type check and the frontend test suite before opening a PR:

```bash
npx tsc --noEmit
npx vitest run
```

(`npm test` runs the same Vitest command, and `npm run build` runs both the type check and the Vite build.) The type check is what actually catches missing/mismatched keys across the four files; there's no separate linter for translation completeness.

If you're not sure your wording fits the context, mention which screen/dialog the string appears in when you open the PR — a screenshot helps.

---

## Übersetzen (Deutsch)

Die Oberfläche gibt es auf Deutsch, Englisch, Spanisch und Französisch. Dieses Dokument erklärt, wie Übersetzungen im Code aufgebaut sind und wie man eine hinzufügt oder korrigiert.

### Wie es funktioniert

Alle übersetzbaren Texte werden durch eine TypeScript-Schnittstelle definiert, `Translations`, in [`src/i18n/types.ts`](../src/i18n/types.ts). Jede Sprache hat dann ihre eigene Datei, die diese Schnittstelle implementiert:

- [`src/i18n/de.ts`](../src/i18n/de.ts) — Deutsch
- [`src/i18n/en.ts`](../src/i18n/en.ts) — Englisch
- [`src/i18n/es.ts`](../src/i18n/es.ts) — Spanisch
- [`src/i18n/fr.ts`](../src/i18n/fr.ts) — Französisch

Weil jede Datei als `Translations` typisiert ist, **müssen alle vier Dateien exakt dieselben Schlüssel definieren.** Fehlt ein Schlüssel in einer Datei oder ist er falsch geschrieben, kompiliert TypeScript nicht – `npx tsc --noEmit` (oder `npm run build`) findet das sofort. [`src/i18n/LanguageContext.tsx`](../src/i18n/LanguageContext.tsx) wählt zur Laufzeit das aktive Wörterbuch und liest Texte über den Hook `useT()` aus.

Zwei Dinge zu den Textwerten selbst:

- **Pluralformen.** Manche Einträge nutzen statt einem einfachen String `PluralForms`: `{ one: '...', other: '...' }`. Die Hilfsfunktion `formatCount()` in `types.ts` wählt `one` bei genau 1 und sonst `other`, und setzt `{count}` ein. Beispiel (aus `de.ts`):
  ```ts
  filesCount: { one: '{count} Datei', other: '{count} Dateien' },
  ```
- **Platzhalter.** Manche Texte enthalten `{platzhalter}`, die zur Laufzeit durch einen Wert ersetzt werden – z. B. `{version}`, `{mode}`, `{count}`, `{spool}`. Die Platzhalter-Namen müssen exakt wie beim gleichen Schlüssel in den anderen Sprachen bleiben; nur der umgebende Text ändert sich. Beispiel (aus `de.ts`):
  ```ts
  infoAppVersionLabel: 'Version {version}',
  ```

### Text hinzufügen oder korrigieren

- **Bestehende Übersetzung korrigieren:** den Schlüssel in der zu korrigierenden Sprachdatei finden (z. B. den deutschen Text in `de.ts` suchen, um den Schlüssel zu finden, dann denselben Schlüssel in `en.ts`/`es.ts`/`fr.ts` anpassen) und den Wert ändern. Vorhandene `{platzhalter}` dabei nicht verändern.
- **Neuen Text hinzufügen** (meist zusammen mit einem neuen Feature): den Schlüssel zur `Translations`-Schnittstelle in `types.ts` hinzufügen, mit kurzem Doc-Kommentar, falls die Verwendung nicht offensichtlich ist (siehe bestehende Beispiele für `{version}`/`{count}`/`{spool}`-Platzhalter), dann denselben Schlüssel mit passendem Wert in **allen vier** Sprachdateien ergänzen. Fehlt er in einer davon, schlägt der Build fehl.

### Änderung prüfen

Vor dem Öffnen eines PRs Typprüfung und Frontend-Testsuite ausführen:

```bash
npx tsc --noEmit
npx vitest run
```

(`npm test` ruft denselben Vitest-Befehl auf, `npm run build` führt Typprüfung und Vite-Build zusammen aus.) Die Typprüfung ist es, die tatsächlich fehlende/abweichende Schlüssel über die vier Dateien hinweg findet; einen separaten Linter für die Vollständigkeit der Übersetzungen gibt es nicht.

Falls du unsicher bist, ob deine Formulierung zum Kontext passt, gib im PR an, in welchem Bildschirm/Dialog der Text erscheint – ein Screenshot hilft.
