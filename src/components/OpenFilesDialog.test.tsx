import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { OpenFilesDialog } from './OpenFilesDialog';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { de } from '../i18n/de';
import type { OpenRequest } from '../lib/api/openRequests';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('./ModelViewer', () => ({ ModelViewer: (props: { externalSourceId?: string }) => <div data-testid="viewer" data-source={props.externalSourceId} /> }));

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const external = (id: string, name = `${id}.3mf`): OpenRequest => ({ kind: 'external', sourceId: id, name, sizeBytes: 2048, extension: 'stl' });
const known = (id: string): OpenRequest => ({ kind: 'catalog', fileId: id });

function open(requests: OpenRequest[], { locked = false, onAdd = vi.fn(), onClose = vi.fn() } = {}) {
  render(<LanguageProvider><ImportLockContext.Provider value={locked}>
    <OpenFilesDialog requests={requests} nameOfCatalogFile={id => (id === '4' ? 'Bekannt.3mf' : undefined)} onAdd={onAdd} onClose={onClose} />
  </ImportLockContext.Provider></LanguageProvider>);
  return { onAdd, onClose };
}

describe('OpenFilesDialog', () => {
  it('shows one external file in the viewer by its source id and adds it on request', () => {
    const { onAdd } = open([external('ext-1', 'Deckel.stl')]);
    const dialog = screen.getByRole('dialog', { name: de.owViewTitle });
    expect(within(dialog).getByText('Deckel.stl')).toBeVisible();
    expect(screen.getByTestId('viewer')).toHaveAttribute('data-source', 'ext-1');
    expect(screen.getByText(de.owNoChange)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: de.owAdd }));
    expect(onAdd).toHaveBeenCalledWith([external('ext-1', 'Deckel.stl')]);
    expect(screen.getByRole('button', { name: de.owAdding })).toBeDisabled();
  });

  it('puts the focus on Close and closes without adding', () => {
    const { onAdd, onClose } = open([external('ext-1')]);
    expect(document.activeElement).toBe(screen.getAllByRole('button', { name: de.owClose }).find(b => b.hasAttribute('data-initial-focus')));
    fireEvent.keyDown(document.activeElement as Element, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
    expect(onAdd).not.toHaveBeenCalled();
  });

  it('blocks adding while an import runs and says why', () => {
    open([external('ext-1')], { locked: true });
    expect(screen.getByRole('button', { name: de.owAdd })).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent(de.owLocked);
  });

  it('lists several files without a viewer, counts the cataloged ones and adds only the new ones', () => {
    const { onAdd } = open([known('4'), external('ext-1', 'A.3mf'), external('ext-2', 'B.stl'), known('9')]);
    expect(screen.getByRole('dialog', { name: de.owListTitle })).toBeVisible();
    expect(screen.queryByTestId('viewer')).not.toBeInTheDocument();
    expect(screen.getByText(de.owHanded.replace('{total}', '4').replace('{known}', '2'))).toBeVisible();
    expect(screen.getByText('Bekannt.3mf')).toBeVisible();
    expect(screen.getByText(de.owCatalogEntry)).toBeVisible();
    expect(screen.getAllByText(de.owNew)).toHaveLength(2);
    fireEvent.click(screen.getByRole('button', { name: de.owAdd }));
    expect(onAdd).toHaveBeenCalledWith([external('ext-1', 'A.3mf'), external('ext-2', 'B.stl')]);
  });

  it('offers nothing to add when every file is already cataloged', () => {
    open([known('4'), known('9')]);
    expect(screen.getByRole('button', { name: de.owAdd })).toBeDisabled();
  });
});
