import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ErrorText } from './ErrorText';

describe('importActive errors', () => {
  it.each([
    ['de', 'Import läuft – bitte warten oder abbrechen.'],
    ['en', 'Import in progress – please wait or cancel it.'],
    ['es', 'Importación en curso: espera o cancélala.'],
    ['fr', 'Importation en cours : veuillez patienter ou l’annuler.'],
  ])('translates expected errors in %s', (language, message) => {
    localStorage.setItem('3mf-katalog-language', language);
    render(<LanguageProvider><ErrorText error={{ message: 'importActive', unexpected: false }} /></LanguageProvider>);
    expect(screen.getByText(message)).toBeTruthy();
    expect(screen.queryByRole('link')).toBeNull();
    localStorage.removeItem('3mf-katalog-language');
  });
});
