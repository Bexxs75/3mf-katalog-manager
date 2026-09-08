import type { ModelFile } from '../types';

interface Props {
  models: ModelFile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

const syncLabel: Record<string, string> = {
  synced: 'aktuell',
  outdated: 'veraltet',
  'local-only': 'nur lokal',
  'cloud-only': 'nur Cloud',
};

export function ModelList({ models, selectedId, onSelect }: Props) {
  return (
    <div className="border border-[var(--line)] rounded overflow-x-auto bg-[var(--panel)]">
      <div
        className="min-w-[680px] grid gap-2.5 items-center px-3 py-2 bg-[var(--panel-2)] border-b border-[var(--line)] font-mono-ui text-[10px] tracking-[0.1em] uppercase text-[var(--ink-3)]"
        style={{ gridTemplateColumns: '62px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px 74px' }}
      >
        <span>Herkunft</span>
        <span>Name</span>
        <span>Tags</span>
        <span>Volumen</span>
        <span>Größe</span>
        <span>Sync</span>
      </div>
      {models.map((m) => (
        <div
          key={m.id}
          onClick={() => onSelect(m.id)}
          className={`min-w-[680px] grid gap-2.5 items-center px-3 py-2 border-b border-[var(--line)] cursor-pointer ${
            m.id === selectedId ? 'bg-[var(--accent-soft)]' : 'hover:bg-[var(--panel-2)]'
          }`}
          style={{ gridTemplateColumns: '62px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px 74px' }}
        >
          <span className="font-mono-ui text-[10px] text-[var(--ink-3)]">{m.origin}</span>
          <span className="text-[13px] font-medium overflow-hidden text-ellipsis whitespace-nowrap">
            {m.name}
          </span>
          <span className="flex gap-1 overflow-hidden">
            {m.tags.map((t) => (
              <span
                key={t}
                className="font-mono-ui text-[10px] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap"
              >
                #{t}
              </span>
            ))}
          </span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">{m.volumeLabel}</span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">{m.filesizeLabel}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-2)]">{syncLabel[m.sync]}</span>
        </div>
      ))}
    </div>
  );
}
