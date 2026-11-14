import { expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as Provider } from '../test/renderWithDiagnostics';
import { DropRejectedNotice } from './DropRejectedNotice';
import { makeFolder } from '../test/factories';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));

it('names the missing folder, says nothing was imported and closes by button', () => {
  const onDismiss = vi.fn();
  render(<Provider><DropRejectedNotice target={{ kind: 'invalid', folder: makeFolder({ name: 'Archiv alt' }), reason: 'missing' }} onDismiss={onDismiss} /></Provider>);
  expect(screen.getByRole('alert')).toHaveTextContent('Der Ordner „Archiv alt“ fehlt auf der Festplatte. Es wurde nichts importiert und nichts verschoben.');
  fireEvent.click(screen.getByRole('button', { name: 'Schließen' }));
  expect(onDismiss).toHaveBeenCalled();
});
it('explains that the catalog folder is no target', () => {
  render(<Provider><DropRejectedNotice target={{ kind: 'invalid', folder: makeFolder({ name: 'Katalog' }), reason: 'catalogRoot' }} onDismiss={() => {}} /></Provider>);
  expect(screen.getByRole('alert')).toHaveTextContent('Der Katalogordner selbst ist kein Ziel');
});
