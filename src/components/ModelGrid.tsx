import type { ModelFile } from '../types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  models: ModelFile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
  onToggleFavorite: (id: string) => void;
}

const originAbbr: Record<string, string> = {
  local: '',
  gdrive: 'GD',
  onedrive: 'OD',
  dropbox: 'DB',
  proton: 'PD',
};

export function ModelGrid({ models, selectedId, onSelect, onContextMenu, onToggleFavorite }: Props) {
  const t = useT();

  return (
    <div className="grid gap-3.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(178px, 1fr))' }}>
      {models.map((m) => (
        <div
          key={m.id}
          onClick={() => onSelect(m.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            onSelect(m.id);
            onContextMenu(m.id, e.clientX, e.clientY);
          }}
          className={`rounded-[4px] overflow-hidden border cursor-pointer ${
            m.id === selectedId ? 'border-[var(--accent)]' : 'border-[var(--line)]'
          }`}
        >
          <div className="relative aspect-square bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
            {m.displayImage ? (
              <img
                src={m.displayImage}
                alt=""
                className="absolute inset-0 w-full h-full object-cover"
              />
            ) : (
              <>
                <div
                  className="absolute inset-0 opacity-90"
                  style={{
                    backgroundImage:
                      'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 9px)',
                  }}
                />
                <div className="absolute inset-0 grid place-items-center">
                  <div className="w-[52px] h-[52px] border border-dashed border-[var(--line-strong)] rotate-45" />
                </div>
                <div className="absolute left-2 bottom-[7px] font-mono-ui text-[9px] tracking-[0.08em] uppercase text-[var(--ink-3)]">
                  {t('previewLabel3d')}
                </div>
              </>
            )}
            {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
              <div className="absolute left-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
                {t('newBadge')}
              </div>
            )}
            {originAbbr[m.origin] && (
              <div className="absolute right-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
                {originAbbr[m.origin]}
              </div>
            )}
            {m.printStatus === 'printed' && (
              <div className="absolute right-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]">
                ✓ {t('printedBadge')}
              </div>
            )}
            <button
              onClick={(e) => {
                e.stopPropagation();
                onToggleFavorite(m.id);
              }}
              aria-label={m.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              className={`absolute left-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border cursor-pointer ${
                m.favorite
                  ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                  : 'border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]'
              }`}
            >
              {m.favorite ? '♥' : '♡'}
            </button>
          </div>
          <div className="flex flex-col gap-1.5 px-2.5 py-2.5 bg-[var(--panel)]">
            <div className="text-[12.5px] font-semibold overflow-hidden text-ellipsis whitespace-nowrap">
              {m.name}
            </div>
            <div className="flex flex-wrap gap-1">
              {m.tags.map((tag) => (
                <span
                  key={tag}
                  className="font-mono-ui text-[10px] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]"
                >
                  #{tag}
                </span>
              ))}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
