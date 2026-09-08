import type { Folder, TagCount, CloudAccount } from '../types';

interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  folders: Folder[];
  activeFolderId: string;
  onFolderSelect: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  onTagSelect: (label: string | null) => void;
  clouds: CloudAccount[];
  onAddCloud: () => void;
}

const originAbbr: Record<string, string> = {
  gdrive: 'GD',
  onedrive: 'OD',
  dropbox: 'DB',
  proton: 'PD',
};

export function Sidebar({
  query,
  onQueryChange,
  folders,
  activeFolderId,
  onFolderSelect,
  tags,
  activeTag,
  onTagSelect,
  clouds,
  onAddCloud,
}: Props) {
  return (
    <aside className="flex-none w-[242px] flex flex-col min-h-0 bg-[var(--panel)] border-r border-[var(--line)]">
      <div className="p-3 pb-2.5 border-b border-[var(--line)]">
        <div className="flex items-center gap-1.5 h-8 px-2.5 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)]">
          <span className="font-mono-ui text-xs text-[var(--ink-3)]">⌕</span>
          <input
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder="Name oder Tag suchen …"
            className="flex-1 min-w-0 border-0 outline-0 bg-transparent text-[var(--ink)] text-[13px]"
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2 py-3">
        <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] px-1.5 pb-2">
          Ordner
        </div>
        {folders.map((f) => (
          <div
            key={f.id}
            onClick={() => onFolderSelect(f.id)}
            className={`flex items-center gap-2 h-8 px-1.5 rounded-[3px] text-[13px] cursor-pointer ${
              f.id === activeFolderId
                ? 'bg-[var(--panel-2)] text-[var(--ink)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
              {f.name}
            </span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{f.count}</span>
          </div>
        ))}

        <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] px-1.5 pt-[18px] pb-2">
          Tags
        </div>
        {tags.map((t) => (
          <div
            key={t.label}
            onClick={() => onTagSelect(activeTag === t.label ? null : t.label)}
            className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer ${
              activeTag === t.label
                ? 'bg-[var(--accent-soft)] text-[var(--accent)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span
              className="w-[7px] h-[7px] rounded-full"
              style={{ background: `oklch(0.62 0.14 ${t.colorHue})` }}
            />
            <span className="flex-1 font-mono-ui text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              #{t.label}
            </span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{t.count}</span>
          </div>
        ))}
      </div>

      <div className="flex-none border-t border-[var(--line)] px-3.5 pt-3 pb-3.5">
        <div className="flex items-center justify-between pb-2.5">
          <span className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            Cloud-Konten
          </span>
          <span
            onClick={onAddCloud}
            className="font-mono-ui text-sm leading-none text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
          >
            +
          </span>
        </div>
        {clouds.map((c) => (
          <div key={c.id} className="flex flex-col gap-1.5 py-1.5">
            <div className="flex items-center gap-2">
              <span className="font-mono-ui text-[10px] px-1 py-0.5 rounded border border-[var(--line-strong)] text-[var(--ink-2)]">
                {originAbbr[c.id] ?? c.abbr}
              </span>
              <span className="flex-1 text-[12.5px] font-medium overflow-hidden text-ellipsis whitespace-nowrap">
                {c.name}
              </span>
              <span
                className={`font-mono-ui text-[10px] ${
                  c.status === 'connected' ? 'text-[var(--ink-3)]' : 'text-[var(--accent)]'
                }`}
              >
                {c.status === 'connected'
                  ? 'verbunden'
                  : c.status === 'error'
                  ? 'Fehler'
                  : 'getrennt'}
              </span>
            </div>
            <div className="flex items-center gap-2 pl-[26px]">
              <div className="flex-1 h-[3px] rounded bg-[var(--line)] overflow-hidden">
                <div
                  className="h-full bg-[var(--accent)]"
                  style={{ width: `${c.usedPercent}%` }}
                />
              </div>
              <span className="font-mono-ui text-[10px] text-[var(--ink-3)] whitespace-nowrap">
                {c.quotaLabel}
              </span>
            </div>
          </div>
        ))}
      </div>
    </aside>
  );
}
