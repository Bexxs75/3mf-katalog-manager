import type { ModelFile } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatVolumeCm3 } from '../i18n/format';

interface Props {
  models: ModelFile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
}

export function ModelList({ models, selectedId, onSelect, onContextMenu }: Props) {
  const { language } = useLanguage();
  const t = useT();

  return (
    <div className="border border-[var(--line)] rounded overflow-x-auto bg-[var(--panel)]">
      <div
        className="min-w-[680px] grid gap-2.5 items-center px-3 py-2 bg-[var(--panel-2)] border-b border-[var(--line)] font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.1em] uppercase text-[var(--ink-3)]"
        style={{ gridTemplateColumns: 'minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px' }}
      >
        <span>{t('columnName')}</span>
        <span>{t('columnTags')}</span>
        <span>{t('columnVolume')}</span>
        <span>{t('columnSize')}</span>
      </div>
      {models.map((m) => (
        <div
          key={m.id}
          onClick={() => onSelect(m.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            onSelect(m.id);
            onContextMenu(m.id, e.clientX, e.clientY);
          }}
          className={`min-w-[680px] grid gap-2.5 items-center px-3 py-2 border-b border-[var(--line)] cursor-pointer ${
            m.id === selectedId ? 'bg-[var(--accent-soft)]' : 'hover:bg-[var(--panel-2)]'
          }`}
          style={{ gridTemplateColumns: 'minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px' }}
        >
          <span className="text-[length:var(--font-size-body)] font-medium overflow-hidden text-ellipsis whitespace-nowrap">
            {m.name}
          </span>
          <span className="flex gap-1 overflow-hidden">
            {m.tags.map((tag) => (
              <span
                key={tag}
                className="font-mono-ui text-[length:var(--font-size-meta)] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap"
              >
                #{tag}
              </span>
            ))}
          </span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">
            {formatVolumeCm3(m.volumeCm3, language)}
          </span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">
            {formatBytes(m.fileSizeBytes, language)}
          </span>
        </div>
      ))}
    </div>
  );
}
