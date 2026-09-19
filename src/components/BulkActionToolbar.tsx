import type { Collection } from '../types';
import { useT } from '../i18n/LanguageContext';

interface BulkActionToolbarProps {
  selectedCount: number;
  confirmBulkDelete: boolean;
  onConfirmBulkDeleteChange: (value: boolean) => void;
  onSelectAllVisible: () => void;
  onClearSelection: () => void;
  onBulkAddToQueue: () => void;
  addToCollectionMenuOpen: boolean;
  onAddToCollectionMenuOpenChange: (value: boolean) => void;
  collections: Collection[];
  onBulkAddToCollection: (collectionId: string) => void;
  activeCollection: string | null;
  onBulkRemoveFromCollection: () => void;
  onBulkSetPrintStatus: (status: 'printed' | 'not_printed') => void;
  onBulkDelete: () => void;
}

export function BulkActionToolbar({
  selectedCount,
  confirmBulkDelete,
  onConfirmBulkDeleteChange,
  onSelectAllVisible,
  onClearSelection,
  onBulkAddToQueue,
  addToCollectionMenuOpen,
  onAddToCollectionMenuOpenChange,
  collections,
  onBulkAddToCollection,
  activeCollection,
  onBulkRemoveFromCollection,
  onBulkSetPrintStatus,
  onBulkDelete,
}: BulkActionToolbarProps) {
  const t = useT();
  return (
    <div className="flex-none flex items-center gap-2 px-4 py-2 border-b border-[var(--line)] bg-[var(--panel-2)]">
      {confirmBulkDelete ? (
        <>
          <span className="text-[12.5px] font-medium text-[var(--ink)]">
            {t('bulkDeleteConfirmQuestion').replace('{count}', String(selectedCount))}
          </span>
          <button
            onClick={() => onConfirmBulkDeleteChange(false)}
            className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('cancel')}
          </button>
          <button
            onClick={onBulkDelete}
            className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
          >
            {t('delete')}
          </button>
        </>
      ) : (
        <>
          <span className="text-[12.5px] font-medium text-[var(--ink)]">
            {t('bulkSelectedCount').replace('{count}', String(selectedCount))}
          </span>
          <button onClick={onSelectAllVisible} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
            {t('selectAllLabel')}
          </button>
          <button onClick={onClearSelection} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
            {t('clearSelectionLabel')}
          </button>
          <span className="flex-1" />
          <button onClick={onBulkAddToQueue} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
            {t('addToQueue')}
          </button>
          <div className="relative">
            <button
              onClick={() => onAddToCollectionMenuOpenChange(!addToCollectionMenuOpen)}
              className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]"
            >
              {t('addToCollectionLabel')}
            </button>
            {addToCollectionMenuOpen && (
              <div className="absolute top-9 left-0 min-w-[220px] max-w-[360px] py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
                {collections.map((c) => (
                  <button
                    key={c.id}
                    title={c.name}
                    onClick={() => {
                      onBulkAddToCollection(c.id);
                      onAddToCollectionMenuOpenChange(false);
                    }}
                    className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer whitespace-nowrap overflow-hidden text-ellipsis"
                  >
                    {c.name}
                  </button>
                ))}
                {collections.length === 0 && (
                  <div className="px-3 py-1.5 font-mono-ui text-[11px] text-[var(--ink-3)]">
                    {t('noCollectionsEmptyState')}
                  </div>
                )}
              </div>
            )}
          </div>
          {activeCollection && (
            <button
              onClick={onBulkRemoveFromCollection}
              className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]"
            >
              {t('removeFromCollectionLabel')}
            </button>
          )}
          <button onClick={() => onBulkSetPrintStatus('printed')} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
            {t('printedBadge')}
          </button>
          <button onClick={() => onBulkSetPrintStatus('not_printed')} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
            {t('notPrintedLabel')}
          </button>
          <button onClick={() => onConfirmBulkDeleteChange(true)} className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-red-400 text-[12.5px] font-semibold cursor-pointer hover:border-red-400">
            {t('delete')}
          </button>
        </>
      )}
    </div>
  );
}
