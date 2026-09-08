import { useMemo, useState } from 'react';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ModelGrid } from './components/ModelGrid';
import { ModelList } from './components/ModelList';
import { DetailPanel } from './components/DetailPanel';
import { useTheme } from './hooks/useTheme';
import type { ModelFile, Folder, TagCount, CloudAccount, ViewMode, SortKey } from './types';

// Beispieldaten. In der echten Anwendung ersetzt durch Daten aus SQLite via Tauri Commands.
const FOLDERS: Folder[] = [
  { id: 'all', name: 'Alle Modelle', count: 128 },
  { id: 'funktionsteile', name: 'Funktionsteile', count: 41 },
  { id: 'ersatzteile', name: 'Ersatzteile', count: 19 },
  { id: 'deko', name: 'Deko & Figuren', count: 33 },
  { id: 'werkzeuge', name: 'Werkzeuge', count: 22 },
];

const TAGS: TagCount[] = [
  { label: 'halterung', count: 14, colorHue: 30 },
  { label: 'ersatzteil', count: 19, colorHue: 150 },
  { label: 'mehrteilig', count: 8, colorHue: 210 },
  { label: 'vase-mode', count: 6, colorHue: 280 },
  { label: 'funktional', count: 22, colorHue: 30 },
  { label: 'miniatur', count: 11, colorHue: 340 },
];

const CLOUDS: CloudAccount[] = [
  { id: 'gdrive', abbr: 'GD', name: 'Google Drive', status: 'connected', usedPercent: 38, quotaLabel: '5,7/15 GB' },
  { id: 'onedrive', abbr: 'OD', name: 'OneDrive', status: 'connected', usedPercent: 62, quotaLabel: '3,1/5 GB' },
  { id: 'dropbox', abbr: 'DB', name: 'Dropbox', status: 'disconnected', usedPercent: 0, quotaLabel: '—' },
  { id: 'proton', abbr: 'PD', name: 'Proton Drive', status: 'connected', usedPercent: 15, quotaLabel: '0,8/5 GB' },
];

const MODELS: ModelFile[] = [
  {
    id: '1', name: 'kabelhalter_v3.3mf', path: 'Funktionsteile / Halterungen', folderId: 'funktionsteile',
    tags: ['halterung', 'funktional', 'petg', 'kabelmanagement'], origin: 'local', sync: 'synced',
    syncTimeLabel: 'vor 2 Std.', volumeLabel: '6,4 cm³', filesizeLabel: '312 KB',
    meta: [
      { label: 'Größe', value: '42 × 38 × 16 mm' },
      { label: 'Volumen', value: '6,4 cm³' },
      { label: 'Objekte', value: '1' },
      { label: 'Material', value: 'PETG' },
      { label: 'Dateigröße', value: '312 KB' },
      { label: 'Importiert', value: '03.09.2026' },
    ],
  },
  {
    id: '2', name: 'zahnrad_modul2.3mf', path: 'Ersatzteile', folderId: 'ersatzteile',
    tags: ['ersatzteil'], origin: 'gdrive', sync: 'synced', syncTimeLabel: 'vor 1 Tag',
    volumeLabel: '2,1 cm³', filesizeLabel: '198 KB',
    meta: [
      { label: 'Größe', value: '28 × 28 × 8 mm' },
      { label: 'Volumen', value: '2,1 cm³' },
      { label: 'Objekte', value: '1' },
      { label: 'Material', value: 'PLA' },
      { label: 'Dateigröße', value: '198 KB' },
      { label: 'Importiert', value: '01.09.2026' },
    ],
  },
  {
    id: '3', name: 'vase_wellenform.3mf', path: 'Deko & Figuren', folderId: 'deko',
    tags: ['vase-mode', 'deko'], origin: 'onedrive', sync: 'outdated', syncTimeLabel: 'vor 5 Tagen',
    volumeLabel: '38,0 cm³', filesizeLabel: '540 KB',
    meta: [
      { label: 'Größe', value: '80 × 80 × 140 mm' },
      { label: 'Volumen', value: '38,0 cm³' },
      { label: 'Objekte', value: '1' },
      { label: 'Material', value: 'PLA Silk' },
      { label: 'Dateigröße', value: '540 KB' },
      { label: 'Importiert', value: '28.08.2026' },
    ],
  },
];

export default function App() {
  const { setting, setTheme } = useTheme();
  const [view, setView] = useState<ViewMode>('grid');
  const [sort, setSort] = useState<SortKey>('name');
  const [query, setQuery] = useState('');
  const [activeFolderId, setActiveFolderId] = useState('all');
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(MODELS[0]?.id ?? null);
  const [models, setModels] = useState<ModelFile[]>(MODELS);

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

  const updateTags = (id: string, next: string[]) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: next } : m)));
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
        onImport={() => {
          // Tauri: Datei-/Ordner-Dialog öffnen, anschließend 3MF Parsing anstoßen
        }}
      />

      <div className="flex-1 flex min-h-0">
        <Sidebar
          query={query}
          onQueryChange={setQuery}
          folders={FOLDERS}
          activeFolderId={activeFolderId}
          onFolderSelect={setActiveFolderId}
          tags={TAGS}
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
              {FOLDERS.find((f) => f.id === activeFolderId)?.name}
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
          onAddTag={(t) => selected && updateTags(selected.id, [...selected.tags, t])}
          onRemoveTag={(t) => selected && updateTags(selected.id, selected.tags.filter((x) => x !== t))}
          onOpenInSlicer={() => {
            // Tauri: Pfad an registrierten Slicer übergeben
          }}
        />
      </div>
    </div>
  );
}
