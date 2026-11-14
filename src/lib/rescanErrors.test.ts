import { expect, it } from 'vitest';
import { formatAppError } from './errorMessage';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

const codes = {
  rescan_file_unavailable: 'rescanFileUnavailable',
  rescan_file_unsupported: 'rescanFileUnsupported',
  rescan_step_unavailable: 'rescanStepUnavailable',
  rescan_file_too_large: 'rescanFileTooLarge',
  rescan_file_invalid: 'rescanFileInvalid',
  rescan_file_changed: 'rescanFileChanged',
} as const;

it.each([['de', de], ['en', en], ['es', es], ['fr', fr]])('translates every rescan answer of the server in %s', (_, dictionary) => {
  for (const [code, key] of Object.entries(codes)) {
    const text = formatAppError(k => dictionary[k], { message: code });
    expect(text).toBe(dictionary[key]);
    expect(text).not.toBe(code);
  }
});
