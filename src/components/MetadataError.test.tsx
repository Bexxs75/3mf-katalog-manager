import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { MetadataError } from './MetadataError';
import { LanguageProvider, useLanguage } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { fillParams } from '../lib/errorMessage';
import { clearMetadataError, getMetadataError, getMetadataErrorDetail, reportMetadataError } from '../lib/metadataErrors';
afterEach(() => { cleanup(); clearMetadataError(); localStorage.clear(); });
const show = () => render(<LanguageProvider><MetadataError /></LanguageProvider>);

it('adds the reload framing to technical errors only', () => {
  show();
  act(() => reportMetadataError({ message: 'kaputt', expected: true }));
  expect(screen.getByRole('alert')).toHaveTextContent(de.webMetadataError);
  act(() => reportMetadataError({ message: 'Dieser Speicherort ist geschützt.', expected: true, plain: true }));
  expect(screen.getByRole('alert')).toHaveTextContent('Dieser Speicherort ist geschützt.');
  expect(screen.getByRole('alert')).not.toHaveTextContent(de.webMetadataError);
});

function Switch() {
  const { setLanguage } = useLanguage();
  return <button onClick={() => setLanguage('en')}>to-english</button>;
}

it('translates a coded error with its params and follows a language change while it stands', () => {
  render(<LanguageProvider><Switch /><MetadataError /></LanguageProvider>);
  act(() => reportMetadataError({ message: 'folderNameForbiddenChar', expected: true, params: [':'], plain: true }));
  expect(screen.getByRole('alert')).toHaveTextContent(fillParams(de.errFolderNameForbiddenChar, [':']));
  expect(screen.getByRole('alert')).not.toHaveTextContent('folderNameForbiddenChar');
  act(() => screen.getByText('to-english').click());
  expect(screen.getByRole('alert')).toHaveTextContent(fillParams(en.errFolderNameForbiddenChar, [':']));
  expect(screen.getByRole('alert')).not.toHaveTextContent(de.errFolderNameForbiddenChar.slice(0, 12));
});

it('keeps the message and params apart from the framing of a technical error', () => {
  show();
  act(() => reportMetadataError({ message: 'folderNameEmpty', expected: true }));
  expect(getMetadataError()).toBe('folderNameEmpty');
  expect(getMetadataErrorDetail()).toEqual({ message: 'folderNameEmpty' });
  act(() => reportMetadataError({ message: 'folderNameForbiddenChar', expected: true, params: ['*'] }));
  expect(getMetadataErrorDetail()).toEqual({ message: 'folderNameForbiddenChar', params: ['*'] });
  act(() => clearMetadataError());
  expect(getMetadataError()).toBe('');
  expect(getMetadataErrorDetail()).toBeNull();
});
