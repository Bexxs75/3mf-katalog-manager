import { useState } from 'react';
import type { ViewMode, SortKey } from '../types';
import type { ThemeSetting } from '../hooks/useTheme';

interface Props {
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortKey;
  onSortChange: (s: SortKey) => void;
  count: number;
  themeSetting: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  onImportFiles: () => void;
  onImportFolder: () => void;
}

const segBase =
  'h-[26px] px-3 rounded-[2px] text-[12.5px] font-medium cursor-pointer transition-colors';
const segActive = 'bg-[var(--accent)] text-[var(--accent-ink)]';
const segInactive = 'text-[var(--ink-2)] hover:text-[var(--ink)]';

export function Header({
  view,
  onViewChange,
  sort,
  onSortChange,
  count,
  themeSetting,
  onThemeChange,
  onImportFiles,
  onImportFolder,
}: Props) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [importMenuOpen, setImportMenuOpen] = useState(false);

  return (
    <header className="flex-none h-[54px] flex items-center gap-[18px] px-[14px] bg-[var(--panel)] border-b border-[var(--line)]">
      <div className="flex items-baseline gap-2 pr-1.5">
        <span className="text-[15px] font-bold tracking-[0.06em] uppercase">
          3MF Katalog
        </span>
        <span className="font-mono-ui text-[11px] text-[var(--accent)] tracking-[0.08em]">
          MANAGER
        </span>
      </div>

      <div className="relative flex">
        <button
          onClick={() => {
            setImportMenuOpen(false);
            onImportFiles();
          }}
          className="flex items-center gap-2 h-8 pl-[13px] pr-3 rounded-l-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[13px] font-semibold cursor-pointer hover:brightness-110"
        >
          <span className="font-mono-ui text-sm leading-none">+</span>
          <span>Importieren</span>
        </button>
        <button
          onClick={() => setImportMenuOpen((o) => !o)}
          aria-label="Weitere Import-Optionen"
          className="flex items-center justify-center w-6 h-8 rounded-r-[3px] border border-l-0 border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] cursor-pointer hover:brightness-110"
        >
          <span className="text-[9px] leading-none">▾</span>
        </button>

        {importMenuOpen && (
          <div className="absolute top-10 left-0 w-[176px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFiles();
              }}
              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              Dateien...
            </button>
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolder();
              }}
              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              Ordner...
            </button>
          </div>
        )}
      </div>

      <div className="flex items-center gap-1.5">
        <span className="font-mono-ui text-[10px] tracking-[0.1em] uppercase text-[var(--ink-3)]">
          Sortieren
        </span>
        <select
          value={sort}
          onChange={(e) => onSortChange(e.target.value as SortKey)}
          className="h-[30px] px-2 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink)] text-[13px] cursor-pointer"
        >
          <option value="name">Name</option>
          <option value="date">Datum</option>
          <option value="size">Dateigröße</option>
          <option value="vol">Volumen</option>
        </select>
      </div>

      <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
        <button
          onClick={() => onViewChange('grid')}
          className={`${segBase} ${view === 'grid' ? segActive : segInactive}`}
        >
          Raster
        </button>
        <button
          onClick={() => onViewChange('list')}
          className={`${segBase} ${view === 'list' ? segActive : segInactive}`}
        >
          Liste
        </button>
      </div>

      <div className="flex-1" />

      <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">
        {count} Dateien
      </span>

      <div className="relative">
        <button
          onClick={() => setSettingsOpen((o) => !o)}
          className="w-8 h-8 grid place-items-center rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-[15px] cursor-pointer hover:text-[var(--ink)] hover:border-[var(--line-strong)]"
        >
          ⚙
        </button>

        {settingsOpen && (
          <div className="absolute top-10 right-0 w-[268px] p-[14px] bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
            <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] mb-2.5">
              Einstellungen
            </div>
            <div className="text-[13px] font-semibold mb-2">Erscheinungsbild</div>
            <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
              {(['system', 'light', 'dark'] as ThemeSetting[]).map((t) => (
                <button
                  key={t}
                  onClick={() => onThemeChange(t)}
                  className={`${segBase} flex-1 ${themeSetting === t ? segActive : segInactive}`}
                >
                  {t === 'system' ? 'System' : t === 'light' ? 'Hell' : 'Dunkel'}
                </button>
              ))}
            </div>
            <div className="mt-2 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">
              {themeSetting === 'system'
                ? 'Folgt automatisch der Systemeinstellung.'
                : `Manuell auf ${themeSetting === 'light' ? 'Hell' : 'Dunkel'} festgelegt.`}
            </div>
          </div>
        )}
      </div>
    </header>
  );
}
