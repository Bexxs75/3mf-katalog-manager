import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { MetadataError } from './MetadataError';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { clearMetadataError, reportMetadataError } from '../lib/metadataErrors';
afterEach(() => { cleanup(); clearMetadataError(); });
const show = () => render(<LanguageProvider><MetadataError /></LanguageProvider>);

it('adds the reload framing to technical errors only', () => {
  show();
  act(() => reportMetadataError({ message: 'kaputt', expected: true }));
  expect(screen.getByRole('alert')).toHaveTextContent(de.webMetadataError);
  act(() => reportMetadataError({ message: 'Dieser Speicherort ist geschützt.', expected: true, plain: true }));
  expect(screen.getByRole('alert')).toHaveTextContent('Dieser Speicherort ist geschützt.');
  expect(screen.getByRole('alert')).not.toHaveTextContent(de.webMetadataError);
});
