import { expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import type { Translations } from './types';

const KEYS = [
  'moveBatchDone', 'moveBatchMoved', 'moveBatchNothing', 'moveBatchSkippedExists', 'moveBatchSkippedThere', 'moveBatchFailed',
  'moveReasonSourceMissing', 'moveReasonFolderMissing', 'moveReasonGone', 'moveReasonSensitive', 'moveReasonError',
  'undoReasonNameExists', 'undoButton', 'undoDoneFiles', 'undoDoneFolderMove', 'undoDoneRename', 'undoFailed', 'undoFilesPartial',
  'dropHereMany', 'moveDialogTitle', 'moveDialogTreeLabel', 'moveDialogHint', 'moveDialogConfirm', 'moveDialogNoFolders',
  'moveBulkLabel', 'moveContextLabel', 'keyboardMove', 'keyboardUndo',
] as const satisfies readonly (keyof Translations)[];

const texts = (value: Translations[keyof Translations]) => typeof value === 'string' ? [value] : [value.one, value.other];
const placeholders = (value: Translations[keyof Translations]) => texts(value).flatMap((text) => text.match(/\{\w+\}/g) ?? []);

it.each(KEYS)('%s exists in German, English, Spanish and French with the same placeholders', (key) => {
  for (const dictionary of [de, en, es, fr]) {
    for (const text of texts(dictionary[key])) expect(text.trim()).not.toBe('');
  }
  const reference = [...new Set(placeholders(de[key]))].sort();
  for (const dictionary of [en, es, fr]) {
    expect([...new Set(placeholders(dictionary[key]))].sort()).toEqual(reference);
  }
});

it('keeps the undo shortcut hint and the toast wording distinct per language', () => {
  expect(new Set([de, en, es, fr].map((dictionary) => dictionary.undoButton)).size).toBe(4);
});
