import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { BulkDeleteFailureBanner } from './BulkDeleteFailureBanner';

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); });

const renderBanner = (names: string[], onClose = vi.fn()) =>
  render(<LanguageProvider><BulkDeleteFailureBanner names={names} onClose={onClose} /></LanguageProvider>);

it('names a single file that could not be deleted', () => {
  renderBanner(['gesperrt']);
  expect(screen.getByRole('alert')).toHaveTextContent('1 Datei konnte nicht gelöscht werden');
  expect(screen.getByRole('alert')).toHaveTextContent('gesperrt');
});

it('shortens a long list of names and dismisses with a real button', () => {
  const onClose = vi.fn();
  renderBanner(['a', 'b', 'c', 'd', 'e'], onClose);
  const alert = screen.getByRole('alert');
  expect(alert).toHaveTextContent('5 Dateien konnten nicht gelöscht werden');
  expect(alert).toHaveTextContent('a, b, c +2 weitere');
  expect(alert).not.toHaveTextContent('d,');
  fireEvent.click(screen.getByRole('button', { name: 'Meldung schließen' }));
  expect(onClose).toHaveBeenCalledTimes(1);
});

it('renders nothing without failures', () => {
  renderBanner([]);
  expect(screen.queryByRole('alert')).toBeNull();
});
