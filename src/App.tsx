import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ModelGrid } from './components/ModelGrid';
import { ModelList } from './components/ModelList';
import { DetailPanel } from './components/DetailPanel';
import { ContextMenu } from './components/ContextMenu';
import { CloudBrowserDialog } from './components/CloudBrowserDialog';
import { useTheme } from './hooks/useTheme';
import type { ModelFile, Folder, TagCount, CloudAccount, Origin, ViewMode, SortKey } from './types';

interface CloudAccountDto {
  id: string;
  name: string;
  status: 'connected' | 'error' | 'disconnected';
}

// usedPercent/quotaLabel sind noch nicht Teil dieses Backends (echte
// Speicherplatz-Abfrage folgt bei Bedarf spaeter) - "–" statt erfundener
// Zahlen.
const toCloudAccount = (dto: CloudAccountDto): CloudAccount => ({
  id: dto.id as Origin,
  abbr: '',
  name: dto.name,
  status: dto.status,
  usedPercent: 0,
  quotaLabel: '—',
});

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
  const [clouds, setClouds] = useState<CloudAccount[]>([]);
  const [connectingCloud, setConnectingCloud] = useState(false);
  const [cloudError, setCloudError] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);
  const [cloudBrowserOpen, setCloudBrowserOpen] = useState(false);

  const refreshFolders = () => invoke<Folder[]>('list_folders').then(setFolders);
  const refreshTags = () => invoke<TagCount[]>('list_tag_counts').then(setTags);
  const refreshClouds = () =>
    invoke<CloudAccountDto[]>('list_cloud_accounts').then((accounts) => setClouds(accounts.map(toCloudAccount)));

  // Weitere Anbieter (OneDrive/Dropbox/Proton) haben noch keinen eigenen
  // Connect-Command im Backend - bis dahin verbindet dieser Handler nur
  // Google Drive, unabhaengig von welcher Zeile/welchem "+" er ausgeloest wird.
  const connectCloud = (id: string) => {
    if (connectingCloud || id !== 'gdrive') return;
    setConnectingCloud(true);
    invoke('connect_google_drive')
      .then(() => {
        setCloudError(null);
        refreshClouds();
      })
      .catch((e) => {
        console.error('[cloud] Google Drive verbinden fehlgeschlagen:', e);
        setCloudError(String(e));
      })
      .finally(() => setConnectingCloud(false));
  };

  const handleCloudImport = (fileIds: string[]) => {
    setCloudBrowserOpen(false);
    invoke<ModelFile[]>('import_from_cloud', { fileIds })
      .then(mergeImported)
      .catch((e) => {
        console.error('[cloud] Import aus Google Drive fehlgeschlagen:', e);
        setCloudError(String(e));
      });
  };

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
    refreshClouds();
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

  useEffect(() => {
    const model = models.find((m) => m.id === selectedId);
    if (!model || model.origin === 'local') return;
    invoke<string>('check_cloud_sync_status', { fileId: model.id })
      .then((status) => {
        setModels((prev) =>
          prev.map((m) => (m.id === model.id ? { ...m, sync: status as ModelFile['sync'] } : m)),
        );
      })
      .catch((e) => console.error('[cloud] Sync-Check fehlgeschlagen:', e));
  }, [selectedId]);

  const filtered = useMemo(() => {
    return models
      .filter((m) => activeFolderId === 'all' || m.folderId === activeFolderId)
      .filter((m) => !activeTag || m.tags.includes(activeTag))
      .filter((m) => !query || m.name.toLowerCase().includes(query.toLowerCase()))
      .sort((a, b) => {
        if (sort === 'name') return a.name.localeCompare(b.name);
        if (sort === 'size') return a.fileSizeBytes - b.fileSizeBytes;
        if (sort === 'date') return b.importedAt.localeCompare(a.importedAt);
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

  const openInSlicer = (_id: string) => {
    // Tauri: Pfad an registrierten Slicer übergeben
  };

  const deleteModel = (id: string) => {
    invoke('delete_file', { fileId: id }).then(() => {
      setModels((prev) => prev.filter((m) => m.id !== id));
      setSelectedId((prev) => (prev === id ? null : prev));
      refreshFolders();
      refreshTags();
    });
  };

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
        cloudDriveConnected={clouds.some((c) => c.id === 'gdrive' && c.status === 'connected')}
        onImportFromCloud={() => setCloudBrowserOpen(true)}
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
          clouds={clouds}
          cloudError={cloudError}
          onAddCloud={() => connectCloud('gdrive')}
          onConnectCloud={connectCloud}
          onDisconnectCloud={(id) => {
            invoke('disconnect_cloud_account', { provider: id })
              .then(() => {
                setCloudError(null);
                refreshClouds();
              })
              .catch((e) => {
                console.error('[cloud] Trennen fehlgeschlagen:', e);
                setCloudError(String(e));
              });
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
              <ModelGrid
                models={filtered}
                selectedId={selectedId}
                onSelect={setSelectedId}
                onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
              />
            ) : (
              <ModelList
                models={filtered}
                selectedId={selectedId}
                onSelect={setSelectedId}
                onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
              />
            )}
          </div>
        </main>

        <DetailPanel
          model={selected}
          onAddTag={(t) => selected && addTag(selected.id, t)}
          onRemoveTag={(t) => selected && removeTag(selected.id, t)}
          onDelete={() => selected && deleteModel(selected.id)}
          onOpenInSlicer={() => selected && openInSlicer(selected.id)}
        />
      </div>

      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onOpenInSlicer={() => openInSlicer(contextMenu.modelId)}
          onDelete={() => deleteModel(contextMenu.modelId)}
        />
      )}

      {cloudBrowserOpen && (
        <CloudBrowserDialog onClose={() => setCloudBrowserOpen(false)} onImport={handleCloudImport} />
      )}
    </div>
  );
}
