import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ViewerErrorCard } from './ViewerErrorCard';
import { makeModelFile } from '../test/factories';
import { ImportLockContext } from '../hooks/ImportLockContext';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../diagnostics/ReportProblemLink', () => ({ ReportProblemLink: () => <button>Problem melden</button> }));
vi.mock('../hooks/useModelImages', () => ({ useModelImages: () => new Map() }));
beforeEach(() => {
  vi.mocked(invoke).mockReset().mockResolvedValue(undefined);
  localStorage.setItem('3mf-katalog-language', 'de');
});
function card(props: Partial<React.ComponentProps<typeof ViewerErrorCard>> = {}) {
  return <LanguageProvider><ViewerErrorCard error={{ message: 'missing', code: 'notFound', unexpected: false }}
    noWebGL={false} compact={false} model={makeModelFile({ name: 'Topf', path: '/Katalog/Topf.3mf' })} {...props} /></LanguageProvider>;
}
describe('viewer recovery actions', () => {
  it('opens the containing directory through the existing command', async () => {
    render(card());
    fireEvent.click(screen.getByRole('button', { name: 'Im Dateimanager öffnen' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('open_in_file_manager', { path: '/Katalog' }));
    expect(screen.getByRole('alert')).toHaveTextContent('/Katalog/Topf.3mf');
  });
  it('preserves the separator of a Windows drive root', async () => {
    render(card({ model: makeModelFile({ path: 'C:\\Topf.3mf' }) }));
    fireEvent.click(screen.getByRole('button', { name: 'Im Dateimanager öffnen' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('open_in_file_manager', { path: 'C:\\' }));
  });
  it('requires the existing catalog-only confirmation before removal', async () => {
    const remove = vi.fn().mockResolvedValue(undefined);
    render(card({ onRemoveFromCatalog: remove }));
    fireEvent.click(screen.getByRole('button', { name: 'Aus dem Katalog entfernen' }));
    expect(remove).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toHaveTextContent('Die Datei bleibt unverändert auf der Festplatte.');
    fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
    expect(remove).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Aus dem Katalog entfernen' }));
    fireEvent.click(screen.getByRole('button', { name: 'Entfernen' }));
    await waitFor(() => expect(remove).toHaveBeenCalledOnce());
  });
  it('retains the import lock on removal', () => {
    render(<ImportLockContext.Provider value={true}>{card({ onRemoveFromCatalog: vi.fn() })}</ImportLockContext.Provider>);
    expect(screen.getByRole('button', { name: 'Aus dem Katalog entfernen' })).toBeDisabled();
  });
  it('retains the thumbnail alongside a compact error card', () => {
    render(card({ compact: true, model: makeModelFile({ name: 'Topf', thumbnailImage: 'data:image/png;base64,eA==' }), onRemoveFromCatalog: vi.fn() }));
    expect(screen.getByRole('img', { name: 'Topf' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Im Dateimanager öffnen' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Aus dem Katalog entfernen' })).not.toBeInTheDocument();
  });
  it('offers a configured slicer and reporting only for unexpected errors', () => {
    const slicer = vi.fn();
    const view = render(card({ error: { message: 'parse error', code: 'unreadable', unexpected: true }, onOpenInSlicer: slicer }));
    fireEvent.click(screen.getByRole('button', { name: 'Im Slicer öffnen' }));
    expect(slicer).toHaveBeenCalledOnce();
    expect(screen.getByRole('button', { name: 'Problem melden' })).toBeInTheDocument();
    view.rerender(card({ error: { message: 'limit', code: 'tooLarge', unexpected: false } }));
    expect(screen.queryByRole('button', { name: 'Im Slicer öffnen' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Problem melden' })).not.toBeInTheDocument();
  });
  it('offers neither Wiki nor reporting for unsupported WebGL', () => {
    render(card({ error: null, noWebGL: true }));
    expect(screen.getByRole('alert')).toHaveTextContent('WebGL');
    expect(screen.queryByRole('button', { name: 'Problem melden' })).not.toBeInTheDocument();
    expect(screen.queryByText(/Wiki/)).not.toBeInTheDocument();
  });
});
