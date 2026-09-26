import { useState, type ReactNode } from 'react';
import type { FilamentCheck, ModelFile } from '../types';
import { useT } from '../i18n/LanguageContext';
import { QueueList } from './QueueList';
import type { ToolCounts, ToolView } from '../lib/toolViews';

export interface ToolsSectionProps {
  queue: ModelFile[];
  onQueueReorder: (orderedIds: string[]) => void;
  onQueueRemove: (id: string) => void;
  onQueueSelect: (id: string) => void;
  queueFilament?: Map<string, FilamentCheck> | null;
  toolView: ToolView | null;
  onToolViewChange: (view: ToolView | null) => void;
  counts: ToolCounts;
  onOpenCleanup: () => void;
  cleanupScanning: boolean;
  cleanupError: string | null;
}

// Line icons (16 px, currentColor) as in the approved mockup.
function Icon({ children }: { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="flex-none"
    >
      {children}
    </svg>
  );
}

const ICONS = {
  queue: <path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />,
  recent: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </>
  ),
  new: (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="M21 21l-4.3-4.3M11 8v6M8 11h6" />
    </>
  ),
  favorites: <path d="M12 20s-7-4.4-9-8.8A4.6 4.6 0 0 1 12 7a4.6 4.6 0 0 1 9 4.2C19 15.6 12 20 12 20z" />,
  duplicates: (
    <>
      <rect x="8" y="8" width="12" height="12" rx="2" />
      <path d="M4 16V6a2 2 0 0 1 2-2h10" />
    </>
  ),
  cleanup: <path d="M3 7h18M5 7l1 13h12l1-13M9 7V4h6v3M10 11v6M14 11v6" />,
};

const rowBase =
  'w-full flex items-center gap-2.5 h-7 px-1.5 rounded-[5px] text-left text-[length:var(--font-size-item)] cursor-pointer disabled:cursor-default disabled:opacity-50';
const rowIdle = 'text-[var(--ink-2)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)]';
const rowActive = 'bg-[var(--accent-soft)] text-[var(--accent)]';

function Count({ value, active }: { value: number; active?: boolean }) {
  return (
    <span
      className={`ml-auto font-mono-ui text-[length:var(--font-size-meta)] ${active ? 'text-[var(--accent)]' : 'text-[var(--ink-3)]'}`}
    >
      {value}
    </span>
  );
}

function Hint({ children }: { children: ReactNode }) {
  return (
    <span aria-hidden="true" className="ml-auto font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)]">
      {children}
    </span>
  );
}

export function ToolsSection(props: ToolsSectionProps) {
  const t = useT();
  const [collapsed, setCollapsed] = useState(false);
  const [queueOpen, setQueueOpen] = useState(true);

  const viewRow = (view: ToolView, icon: ReactNode, label: string, count: number) => {
    const active = props.toolView === view;
    return (
      <button
        type="button"
        aria-pressed={active}
        onClick={() => props.onToolViewChange(active ? null : view)}
        className={`${rowBase} ${active ? rowActive : rowIdle}`}
      >
        <Icon>{icon}</Icon>
        <span>{label}</span>
        <Count value={count} active={active} />
      </button>
    );
  };

  return (
    <div>
      <button
        type="button"
        aria-expanded={!collapsed}
        onClick={() => setCollapsed((c) => !c)}
        className="w-full flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer bg-transparent border-0 text-left"
      >
        <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
          {t('toolsHeading')}
        </span>
        <span className="font-mono-ui text-[length:var(--font-size-label)] leading-none text-[var(--ink-3)]">
          {collapsed ? '▾' : '▴'}
        </span>
      </button>
      {!collapsed && (
        <div className="flex flex-col gap-px">
          <button
            type="button"
            aria-expanded={queueOpen}
            onClick={() => setQueueOpen((o) => !o)}
            className={`${rowBase} ${rowIdle}`}
          >
            <Icon>{ICONS.queue}</Icon>
            <span>{t('queueHeading')}</span>
            <Count value={props.queue.length} />
          </button>
          {queueOpen && (
            <div className="ml-[22px] pl-1.5 border-l border-[var(--line)] mb-1">
              <QueueList
                queue={props.queue}
                onQueueReorder={props.onQueueReorder}
                onQueueRemove={props.onQueueRemove}
                onQueueSelect={props.onQueueSelect}
                queueFilament={props.queueFilament}
              />
            </div>
          )}
          {viewRow('recent', ICONS.recent, t('toolRecent'), props.counts.recent)}
          {viewRow('new', ICONS.new, t('toolNew'), props.counts.new)}
          {viewRow('favorites', ICONS.favorites, t('toolFavorites'), props.counts.favorites)}
          {viewRow('duplicates', ICONS.duplicates, t('toolDuplicates'), props.counts.duplicateGroups)}
          <button
            type="button"
            disabled={props.cleanupScanning}
            onClick={props.onOpenCleanup}
            className={`${rowBase} ${rowIdle}`}
          >
            <Icon>{ICONS.cleanup}</Icon>
            <span>{t('cleanupDialogTitle')}</span>
            <Hint>↗</Hint>
          </button>
          {props.cleanupError && (
            <div role="alert" className="px-1.5 pl-[32px] text-[length:var(--font-size-meta)] text-[var(--crit)]">
              {props.cleanupError}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
