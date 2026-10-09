import { beforeEach, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { useCollapsedFolders } from '../hooks/useCollapsedFolders';
import { makeFolder, makeModelFile } from '../test/factories';
import { GroupedModelGrid } from './GroupedModelGrid';
import { GroupedModelList } from './GroupedModelList';

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
it.each(['grid', 'list'])('marks valid targets including empty groups in %s view', (view) => {
  function Harness({ dragging = false, over = null }: { dragging?: boolean; over?: string | null }) {
    const collapsedFolders = useCollapsedFolders();
    const Component = view === 'grid' ? GroupedModelGrid : GroupedModelList;
    return <Component models={[makeModelFile({ folderId: 'source' })]} folders={[makeFolder({ id: 'source', name: 'Source' }), makeFolder({ id: 'empty', name: 'Empty' })]}
      selectedId={null} onSelect={vi.fn()} onOpenDetail={vi.fn()} onContextMenu={vi.fn()} onToggleFavorite={vi.fn()} selectedForBulk={new Set()} onToggleBulkSelect={vi.fn()}
      displayPreference="thumbnail" draggedFileId={dragging ? 'model-1' : null} draggedFileFolderId="source" draggedFolderId={null} dragOverFolderId={over}
      onDragFolderStart={vi.fn()} onFolderMouseEnter={vi.fn()} onFolderMouseLeave={vi.fn()} collapsedFolders={collapsedFolders} />;
  }
  const ui = (dragging = false, over: string | null = null) => <LanguageProvider><UiDensityProvider><Harness dragging={dragging} over={over} /></UiDensityProvider></LanguageProvider>;
  const { rerender } = render(ui());
  expect(screen.getByText(makeModelFile().name)).toHaveClass('select-none');
  expect(screen.queryByText('Empty')).toBeNull();
  rerender(ui(true));
  expect(screen.getByText('Empty').parentElement).toHaveClass('border-dashed');
  expect(screen.getByText('Source').parentElement).not.toHaveClass('border-dashed');
  rerender(ui(true, 'empty'));
  expect(screen.getByText('Empty').parentElement).toHaveClass('shadow-[0_0_0_2px_var(--accent-soft)]');
  expect(screen.getByText('hierher')).toBeVisible();
  rerender(ui(true, 'source'));
  expect(screen.queryByText('hierher')).toBeNull();
});
