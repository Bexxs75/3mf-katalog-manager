import { Icon } from './Icon';
import { useRef, useState } from 'react';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import type { Collection } from '../types';
import { useT, useFormatCount } from '../i18n/LanguageContext';


interface Props {
  collections: Collection[];
  onSelect: (id: string) => void;
  onCreate: (name: string) => void;
  onRename: (id: string, name: string) => void;
  onDelete: (id: string) => void;
  onBack: () => void;
}

export function CollectionsGallery({ collections, onSelect, onCreate, onRename, onDelete, onBack }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const deleteTrigger = useRef<HTMLButtonElement | null>(null);
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
      <button
        type="button"
        onClick={onBack}
        title={t('backToCatalog')}
        className="mb-4 inline-flex items-center gap-2 h-9 px-3 rounded-md border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] hover:border-[var(--line-strong)] hover:text-[var(--ink)] cursor-pointer"
      >
        <svg viewBox="0 0 24 24" className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M19 12H5M11 18l-6-6 6-6" />
        </svg>
        <span className="font-medium text-body">{t('backToCatalog')}</span>
      </button>
      {collections.length === 0 && !creating ? (
        <p className="font-medium tabular-nums text-small text-[var(--ink-3)]">{t('noCollectionsEmptyState')}</p>
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
                className="bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-body"
              />
            ) : (
              <div className="flex items-center justify-between gap-2">
                <span className="text-body font-semibold text-[var(--ink)] truncate">{c.name}</span>
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
                    <Icon name="edit" size={14} />
                  </span>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      deleteTrigger.current = e.currentTarget;
                      setDeletingId(c.id);
                    }}
                    aria-label={t('deleteCollectionConfirmQuestion')}
                    className="w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                  >
                    <Icon name="close" size={14} />
                  </button>
                </div>
              </div>
            )}
            <span className="font-medium tabular-nums text-caption text-[var(--ink-3)]">
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
              className="bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-body"
            />
          </div>
        ) : (
          <button
            onClick={() => setCreating(true)}
            className="rounded-[10px] border-2 border-dashed border-[var(--line-strong)] p-4 flex items-center justify-center text-body font-semibold text-[var(--ink-2)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] min-h-[76px]"
           aria-label={t('createCollectionLabel')}>
            <Icon name="plus" size={14} /> {t('createCollectionLabel').replace(/^\+\s*/, '')}
          </button>
        )}
      </div>
      {deletingId !== null && <CatalogActionDialog title={t('deleteCollectionConfirmQuestion')} returnFocus={deleteTrigger} onClose={() => setDeletingId(null)}>
        <div className="flex justify-end gap-2">
          <button data-initial-focus className={catalogActionButton} onClick={() => setDeletingId(null)}>{t('cancel')}</button>
          <button className={catalogActionButton} onClick={() => { onDelete(deletingId); setDeletingId(null); }}>{t('delete')}</button>
        </div>
      </CatalogActionDialog>}
    </div>
  );
}
