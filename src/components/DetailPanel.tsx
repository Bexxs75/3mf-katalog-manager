import { useEffect, useState } from 'react';
import type { ModelFile } from '../types';
import { ModelViewer } from './ModelViewer';

interface Props {
  model: ModelFile | null;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onOpenInSlicer: () => void;
}

const syncLabel: Record<string, string> = {
  synced: 'Aktuell',
  outdated: 'Veraltet',
  'local-only': 'Nur lokal',
  'cloud-only': 'Nur Cloud',
};

export function DetailPanel({ model, onAddTag, onRemoveTag, onDelete, onOpenInSlicer }: Props) {
  const [draft, setDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => setConfirmDelete(false), [model?.id]);

  if (!model) {
    return (
      <aside className="flex-none w-[336px] flex items-center justify-center bg-[var(--panel)] border-l border-[var(--line)] text-[var(--ink-3)] text-[13px] px-6 text-center">
        Wähle ein Modell aus, um Details, Vorschau und Tags zu sehen.
      </aside>
    );
  }

  const submitDraft = () => {
    const t = draft.trim().replace(/^#/, '');
    if (t) onAddTag(t);
    setDraft('');
  };

  return (
    <aside className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
      <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
        <div className="text-[14.5px] font-semibold leading-tight break-words">{model.name}</div>
        <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)] pt-1.5">{model.path}</div>
      </div>

      <div className="flex-1 overflow-y-auto">
        <div className="relative aspect-[4/3] bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 11px)',
            }}
          />
          <ModelViewer key={model.id} fileId={model.id} />
          <div className="absolute left-2.5 bottom-2 font-mono-ui text-[9.5px] tracking-[0.08em] uppercase text-[var(--ink-3)] pointer-events-none">
            Ziehen zum Drehen
          </div>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span
            className={`w-2 h-2 rounded-full ${
              model.sync === 'synced' ? 'bg-[var(--accent)]' : 'bg-[var(--ink-3)]'
            }`}
          />
          <span className="flex-1 text-[12.5px] font-medium">{syncLabel[model.sync]}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">{model.syncTimeLabel}</span>
        </div>

        <div className="px-4 pt-3.5 pb-1">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
            Metadaten
          </div>
          {model.meta.map((row) => (
            <div
              key={row.label}
              className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]"
            >
              <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
                {row.label}
              </span>
              <span className="flex-1 font-mono-ui text-xs text-right">{row.value}</span>
            </div>
          ))}
        </div>

        <div className="px-4 pt-[18px] pb-5">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2.5">
            Hashtags
          </div>
          <div className="flex flex-wrap gap-1.5">
            {model.tags.map((t) => (
              <span
                key={t}
                className="inline-flex items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11.5px]"
              >
                #{t}
                <span
                  onClick={() => onRemoveTag(t)}
                  className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                >
                  ✕
                </span>
              </span>
            ))}
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submitDraft()}
              placeholder="Tag hinzufügen"
              className="h-6 w-[118px] px-2.5 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-[11.5px]"
            />
          </div>
        </div>
      </div>

      <div className="flex-none flex gap-2 px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        {confirmDelete ? (
          <>
            <span className="flex-1 flex items-center text-[12.5px] font-medium text-[var(--ink)]">
              Eintrag löschen?
            </span>
            <button
              onClick={() => setConfirmDelete(false)}
              className="flex-none h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              Abbrechen
            </button>
            <button
              onClick={() => {
                setConfirmDelete(false);
                onDelete();
              }}
              className="flex-none h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
            >
              Löschen
            </button>
          </>
        ) : (
          <>
            <button
              onClick={onOpenInSlicer}
              className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              In Slicer öffnen
            </button>
            <button className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-mono-ui cursor-pointer">
              ↻
            </button>
            <button
              onClick={() => setConfirmDelete(true)}
              aria-label="Eintrag löschen"
              className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-mono-ui cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              ✕
            </button>
          </>
        )}
      </div>
    </aside>
  );
}
