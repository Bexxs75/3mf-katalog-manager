import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ModelGrid } from './components/ModelGrid';
import { ModelList } from './components/ModelList';
import { DetailPanel } from './components/DetailPanel';
import { useTheme } from './hooks/useTheme';
import type { ModelFile, Folder, TagCount, CloudAccount, ViewMode, SortKey } from './types';

// Cloud-Anbindung ist noch nicht implementiert (spätere Phase) – Beispieldaten bleiben bis dahin.
const CLOUDS: CloudAccount[] = [
  { id: 'gdrive', abbr: 'GD', name: 'Google Drive', status: 'connected', usedPercent: 38, quotaLabel: '5,7/15 GB' },
  { id: 'onedrive', abbr: 'OD', name: 'OneDrive', status: 'connected', usedPercent: 62, quotaLabel: '3,1/5 GB' },
  { id: 'dropbox', abbr: 'DB', name: 'Dropbox', status: 'disconnected', usedPercent: 0, quotaLabel: '—' },
  { id: 'proton', abbr: 'PD', name: 'Proton Drive', status: 'connected', usedPercent: 15, quotaLabel: '0,8/5 GB' },
];

export default function App() {
  const { setting, setTheme } = useTheme();
  const [view, setView] = useState<ViewMode>('grid');
  const [sort, setSort] = useState<SortKey>('name');
  const [query, setQuery] = useState('');
  const [activeFolderId, setActiveFolderId] = useState('all');
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [models, setModels] = useState<ModelFile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);

  const refreshFolders = () => invoke<Folder[]>('list_folders').then(setFolders);
  const refreshTags = () => invoke<TagCount[]>('list_tag_counts').then(setTags);

  const mergeImported = (files: ModelFile[]) => {
    if (!files.length) return;
    setModels((prev) => [...prev, ...files]);
    setSelectedId(files[files.length - 1].id);
    refreshFolders();
    refreshTags();
  };

  useEffect(() => {
    invoke<ModelFile[]>('list_files').then((files) => {
      setModels(files);
      setSelectedId((prev) => prev ?? files[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
  }, []);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== 'drop') return;
      invoke<ModelFile[]>('import_dropped', { paths: event.payload.paths }).then(mergeImported);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const filtered = useMemo(() => {
    return models
      .filter((m) => activeFolderId === 'all' || m.folderId === activeFolderId)
      .filter((m) => !activeTag || m.tags.includes(activeTag))
      .filter((m) => !query || m.name.toLowerCase().includes(query.toLowerCase()))
      .sort((a, b) => {
        if (sort === 'name') return a.name.localeCompare(b.name);
        if (sort === 'size') return a.filesizeLabel.localeCompare(b.filesizeLabel);
        return 0;
      });
  }, [models, activeFolderId, activeTag, query, sort]);

  const selected = models.find((m) => m.id === selectedId) ?? null;

  const setLocalTags = (id: string, next: string[]) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: next } : m)));
  };

  const addTag = (id: string, tag: string) => {
    const current = models.find((m) => m.id === id);
    if (!current || current.tags.includes(tag)) return;
    setLocalTags(id, [...current.tags, tag]);
    invoke('add_tag', { fileId: id, tag }).then(refreshTags);
  };

  const removeTag = (id: string, tag: string) => {
    const current = models.find((m) => m.id === id);
    if (!current) return;
    setLocalTags(id, current.tags.filter((t) => t !== tag));
    invoke('remove_tag', { fileId: id, tag }).then(refreshTags);
  };

  const importFiles = () => invoke<ModelFile[]>('import_files').then(mergeImported);
  const importFolder = () => invoke<ModelFile[]>('import_folder').then(mergeImported);

  return (
    <div
      className="h-screen min-h-[620px] flex flex-col bg-[var(--bg)] text-[var(--ink)] overflow-hidden"
      style={{ fontSize: 14 }}
    >
      <Header
        view={view}
        onViewChange={setView}
        sort={sort}
        onSortChange={setSort}
        count={filtered.length}
        themeSetting={setting}
        onThemeChange={setTheme}
        onImportFiles={importFiles}
        onImportFolder={importFolder}
      />

      <div className="flex-1 flex min-h-0">
        <Sidebar
          query={query}
          onQueryChange={setQuery}
          folders={folders}
          activeFolderId={activeFolderId}
          onFolderSelect={setActiveFolderId}
          tags={tags}
          activeTag={activeTag}
          onTagSelect={setActiveTag}
          clouds={CLOUDS}
          onAddCloud={() => {
            // OAuth2 Flow für weiteren Cloud-Anbieter starten
          }}
        />

        <main className="flex-1 min-w-0 flex flex-col min-h-0">
          <div className="flex-none h-[38px] flex items-center gap-2.5 px-4 border-b border-[var(--line)] bg-[var(--bg)]">
            <span className="font-mono-ui text-[11px] text-[var(--ink-2)]">
              {folders.find((f) => f.id === activeFolderId)?.name}
            </span>
            {activeTag && (
              <span
                onClick={() => setActiveTag(null)}
                className="flex items-center gap-1.5 h-[22px] px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11px] cursor-pointer"
              >
                #{activeTag} ✕
              </span>
            )}
          </div>

          <div className="flex-1 overflow-y-auto p-4">
            {view === 'grid' ? (
              <ModelGrid models={filtered} selectedId={selectedId} onSelect={setSelectedId} />
            ) : (
              <ModelList models={filtered} selectedId={selectedId} onSelect={setSelectedId} />
            )}
          </div>
        </main>

        <DetailPanel
          model={selected}
          onAddTag={(t) => selected && addTag(selected.id, t)}
          onRemoveTag={(t) => selected && removeTag(selected.id, t)}
          onOpenInSlicer={() => {
            // Tauri: Pfad an registrierten Slicer übergeben
          }}
        />
      </div>
    </div>
  );
}
