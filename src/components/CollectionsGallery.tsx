import { useState } from 'react';
import type { Collection } from '../types';
import { useT } from '../i18n/LanguageContext';
import { formatCount } from '../i18n/types';

interface Props {
  collections: Collection[];
  onSelect: (id: string) => void;
  onCreate: (name: string) => void;
  onRename: (id: string, name: string) => void;
  onDelete: (id: string) => void;
}

export function CollectionsGallery({ collections, onSelect, onCreate, onRename, onDelete }: Props) {
  const t = useT();
  const [creating, setCreating] = useState(false);
  const [nameDraft, setNameDraft] = useState('');
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');

  const submitCreate = () => {
    const value = nameDraft.trim();
    if (value) onCreate(value);
    setNameDraft('');
    setCreating(false);
  };

  const submitRename = (id: string) => {
    const value = renameDraft.trim();
    if (value) onRename(id, value);
    setRenamingId(null);
  };

  return (
    <div className="flex-1 overflow-y-auto p-4">
      {collections.length === 0 && !creating ? (
        <p className="font-mono-ui text-[12.5px] text-[var(--ink-3)]">{t('noCollectionsEmptyState')}</p>
      ) : null}
      <div className="grid gap-3.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))' }}>
        {collections.map((c) => (
          <div
            key={c.id}
            className="rounded-[10px] overflow-hidden cursor-pointer bg-[var(--panel)] shadow-[var(--shadow)] border-2 border-transparent hover:border-[var(--line-strong)] p-4 flex flex-col gap-2"
            onClick={() => onSelect(c.id)}
          >
            {renamingId === c.id ? (
              <input
                autoFocus
                value={renameDraft}
                onClick={(e) => e.stopPropagation()}
                onChange={(e) => setRenameDraft(e.target.value)}
                onBlur={() => submitRename(c.id)}
                onKeyDown={(e) => e.key === 'Enter' && submitRename(c.id)}
                className="bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-[13px]"
              />
            ) : (
              <div className="flex items-center justify-between gap-2">
                <span className="text-[14px] font-semibold text-[var(--ink)] truncate">{c.name}</span>
                <div className="flex items-center gap-1 flex-none">
                  <span
                    onClick={(e) => {
                      e.stopPropagation();
                      setRenamingId(c.id);
                      setRenameDraft(c.name);
                    }}
                    aria-label={t('renameCollectionAria')}
                    className="w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                  >
                    ✎
                  </span>
                  <span
                    onClick={(e) => {
                      e.stopPropagation();
                      if (window.confirm(t('deleteCollectionConfirmQuestion'))) onDelete(c.id);
                    }}
                    aria-label={t('deleteCollectionConfirmQuestion')}
                    className="w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                  >
                    ✕
                  </span>
                </div>
              </div>
            )}
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">
              {formatCount(t('modelCountLabel'), c.modelCount)}
            </span>
          </div>
        ))}

        {creating ? (
          <div className="rounded-[10px] border-2 border-dashed border-[var(--line-strong)] p-4 flex flex-col gap-2">
            <input
              autoFocus
              value={nameDraft}
              onChange={(e) => setNameDraft(e.target.value)}
              onBlur={submitCreate}
              onKeyDown={(e) => {
                if (e.key === 'Enter') submitCreate();
                if (e.key === 'Escape') {
                  setCreating(false);
                  setNameDraft('');
                }
              }}
              placeholder={t('newCollectionPlaceholder')}
              className="bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-[13px]"
            />
          </div>
        ) : (
          <button
            onClick={() => setCreating(true)}
            className="rounded-[10px] border-2 border-dashed border-[var(--line-strong)] p-4 flex items-center justify-center text-[13px] font-semibold text-[var(--ink-2)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] min-h-[76px]"
          >
            {t('createCollectionLabel')}
          </button>
        )}
      </div>
    </div>
  );
}
