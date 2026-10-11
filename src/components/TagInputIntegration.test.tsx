import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DetailPanel } from './DetailPanel';
import { ModelDetailPage } from './ModelDetailPage';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => null }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => ({}) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));

afterEach(() => localStorage.clear());

describe('tag inputs in both detail views', () => {
  it.each(['compact', 'comfort', 'page'] as const)('selects an existing tag in %s', (view) => {
    localStorage.setItem('3mf-katalog-density', view === 'comfort' ? 'comfort' : 'compact');
    const onAddTag = vi.fn();
    const onClose = vi.fn();
    const props = {
      model: makeModelFile({ tags: ['bereits'] }), allTags: ['bereits', 'Gehäuse'], onAddTag,
      onRemoveTag: vi.fn(), onDelete: vi.fn(), onTogglePrintStatus: vi.fn(), onToggleFavorite: vi.fn(),
      onToggleQueue: vi.fn(), onUploadImage: vi.fn(), onSnapshotCaptured: vi.fn(), onSetSourceUrl: vi.fn(),
      onOpenInSlicer: vi.fn(), slicerError: null,
    };
    render(<LanguageProvider><UiDensityProvider>
      {view === 'page' ? <ModelDetailPage hasPrevious={false} hasNext={false} {...props} onClose={onClose} onRescanMetadata={vi.fn()}
        onAddToCollection={vi.fn()} collections={[]} slicers={[]} rescanError={null}
        rescanSuccess={false} displayPreference="thumbnail" /> : <DetailPanel {...props} />}
    </UiDensityProvider></LanguageProvider>);
    const input = screen.getByRole('combobox');
    expect(input).toHaveAttribute('maxlength', '100');
    fireEvent.change(input, { target: { value: 'geh' } });
    expect(screen.getByRole('listbox', { name: 'Tag-Vorschläge' })).toBeInTheDocument();
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAddTag).toHaveBeenCalledWith('Gehäuse');
    fireEvent.change(input, { target: { value: 'geh' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    expect(input).toHaveValue('geh');
    fireEvent.keyDown(input, { key: 'Escape' });
    if (view === 'page') expect(onClose).toHaveBeenCalledOnce();
  });
});
