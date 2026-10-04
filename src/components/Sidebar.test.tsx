import { expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Sidebar } from './Sidebar';

it('shows search focus on the wrapper, including programmatic focus', () => {
  render(<LanguageProvider><Sidebar
    query="" onQueryChange={vi.fn()} queue={[]} onQueueReorder={vi.fn()}
    onQueueRemove={vi.fn()} onQueueSelect={vi.fn()} folders={[]} totalModelCount={0}
    activeFolderId="all" onFolderSelect={vi.fn()} onCreateFolder={vi.fn()}
    tags={[]} activeTag={null} onTagSelect={vi.fn()} collections={[]}
    activeCollection={null} collectionsGalleryOpen={false} onSelectCollection={vi.fn()}
    onOpenCollectionsGallery={vi.fn()} onCreateCollection={vi.fn()} toolView={null}
    onToolViewChange={vi.fn()} toolCounts={{ recent: 0, new: 0, favorites: 0, duplicateGroups: 0 }}
    onOpenCleanup={vi.fn()} cleanupScanning={false} cleanupError={null}
  /></LanguageProvider>);
  const search = screen.getByRole('textbox');
  search.focus();
  expect(search).toHaveFocus();
  expect(search.parentElement).toHaveClass('focus-within:border-[var(--accent)]');
});
