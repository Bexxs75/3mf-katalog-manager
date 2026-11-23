import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
vi.mock('../lib/transport', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock('./ModelPreview', () => ({ ModelPreview: () => null }));
vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn().mockResolvedValue([]) }));
import { TrashView } from './TrashView';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ModelLayoutProvider } from '../hooks/ModelLayoutContext';
import { makeModelFile } from '../test/factories';
import { authenticationFailed, login } from '../lib/webSession';

beforeEach(async () => {
  vi.stubEnv('VITE_TARGET', 'web');
  localStorage.clear();
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'csrf', mustChangePassword: false })));
  await login('test');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.unstubAllEnvs(); vi.clearAllMocks(); });

function show(desktopTrash: boolean) {
  const model = makeModelFile({ id: '1', name: 'Desktop.stl', deletedAt: '2026-01-02T00:00:00Z', desktopTrash });
  render(<LanguageProvider><UiDensityProvider><ModelLayoutProvider>
    <TrashView trashModels={[model]} selectedId="1" onSelect={vi.fn()} view="grid" confirmEmptyTrash={false}
      onConfirmEmptyTrashChange={vi.fn()} onEmptyTrash={vi.fn()} onRestore={vi.fn()} onDeletePermanently={vi.fn()}
      displayPreference="thumbnail" />
  </ModelLayoutProvider></UiDensityProvider></LanguageProvider>);
}

it('shows desktop entries, explains restore and offers permanent deletion', () => {
  show(true);
  expect(screen.getByText(/Wiederherstellen ist nur am Desktop möglich/)).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Wiederherstellen' })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Endgültig löschen' }));
  expect(screen.getByRole('dialog')).toBeInTheDocument();
});

it('keeps restore available for web trash entries', () => {
  show(false);
  expect(screen.queryByText(/Wiederherstellen ist nur am Desktop möglich/)).toBeNull();
  expect(screen.getByRole('button', { name: 'Wiederherstellen' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Endgültig löschen' })).toBeInTheDocument();
});
