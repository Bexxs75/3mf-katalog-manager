import { WebFileActions } from './WebFileActions';
import { isReadOnly } from '../lib/readOnly';
import { useImportLock } from '../hooks/ImportLockContext';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { DetailPanel } from './DetailPanel';
import { useT } from '../i18n/LanguageContext';
import { useMemo } from 'react';
import { BundleCardsContext } from '../hooks/BundleCardsContext';
import { indexBundles } from '../lib/bundles';
import type { Bundle, ModelFile, ViewMode } from '../types';
import type { DisplayPreference } from '../hooks/useDisplayPreference';

interface TrashViewProps {
  trashModels: ModelFile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  view: ViewMode;
  confirmEmptyTrash: boolean;
  onConfirmEmptyTrashChange: (value: boolean) => void;
  onEmptyTrash: () => void;
  onRestore: (id: string) => void;
  onDeletePermanently: (id: string) => void;
  displayPreference: DisplayPreference;
  /** Bundles of the catalog: a file in the trash stays a member and says which bundle it was part of. */
  bundles?: Bundle[];
}

export function TrashView({
  trashModels,
  selectedId,
  onSelect,
  view,
  confirmEmptyTrash,
  onConfirmEmptyTrashChange,
  onEmptyTrash,
  onRestore,
  onDeletePermanently,
  displayPreference,
  bundles,
}: TrashViewProps) {
  const { lockProps } = useImportLock();
  const t = useT();
  // The trash doesn't support folder grouping (deleted files no longer
  // "belong" to an active folder context) - falls back to the respective
  // flat display if the shared toggle is set to a grouped option
  // while the trash view is shown.
  const effectiveIsGrid = view === 'grid' || view === 'groupedGrid';
  // Files of a bundle stay single cards here (so each can be restored or deleted on its own) and are marked.
  const bundleCards = useMemo(() => ({ bundles: bundles ?? [], collapse: false, trash: true }), [bundles]);
  const selectedModel = trashModels.find((m) => m.id === selectedId) ?? null;
  const selectedBundle = useMemo(() => {
    if (!selectedModel || !bundles) return null;
    const bundle = indexBundles(bundles).get(selectedModel.id);
    if (!bundle) return null;
    return { name: bundle.name, kind: bundle.kind, label: bundle.members.find(member => member.fileId === selectedModel.id)?.label ?? null };
  }, [bundles, selectedModel]);
  return (
    <BundleCardsContext.Provider value={bundleCards}>
    <div className="flex flex-1 min-h-0">
      <main className="flex-1 min-w-0 flex flex-col">
        <div className="flex-none flex items-center justify-between px-4 py-3 border-b border-[var(--line)]">
          <h1 className="text-title font-semibold">{t('trashHeading')}</h1>
          {import.meta.env.VITE_TARGET === 'web' ? <WebFileActions empty /> : confirmEmptyTrash ? (
            <div className="flex items-center gap-2">
              <span className="text-small font-medium text-[var(--ink)]">
                {t('emptyTrashConfirmQuestion')}
              </span>
              <>{!isReadOnly() && <button
                onClick={() => onConfirmEmptyTrashChange(false)}
                className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>}</>
              <>{!isReadOnly() && <button {...lockProps}
                onClick={onEmptyTrash}
                className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer"
              >
                {t('emptyTrashButton')}
              </button>}</>
            </div>
          ) : (
            <>{!isReadOnly() && <button
              onClick={() => onConfirmEmptyTrashChange(true)}
              disabled={trashModels.length === 0} {...lockProps}
              className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer disabled:opacity-50 hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('emptyTrashButton')}
            </button>}</>
          )}
        </div>
        <div className="flex-1 overflow-y-auto overscroll-contain p-4">
          {trashModels.length === 0 ? (
            <p className="font-medium tabular-nums text-small text-[var(--ink-3)]">{t('trashEmptyState')}</p>
          ) : effectiveIsGrid ? (
            <ModelGrid
              models={trashModels}
              selectedId={selectedId}
              onSelect={onSelect}
              onOpenDetail={() => {}}
              onContextMenu={() => {}}
              onToggleFavorite={() => {}}
              selectedForBulk={new Set()}
              onToggleBulkSelect={() => {}}
              displayPreference={displayPreference}
              readOnly
            />
          ) : (
            <ModelList
              models={trashModels}
              selectedId={selectedId}
              onSelect={onSelect}
              onOpenDetail={() => {}}
              onContextMenu={() => {}}
              selectedForBulk={new Set()}
              onToggleBulkSelect={() => {}}
              readOnly
            />
          )}
        </div>
      </main>
      <DetailPanel
        allTags={[]}
        model={selectedModel}
        trashMode
        trashBundle={selectedBundle}
        onRestore={() => selectedId && onRestore(selectedId)}
        onDeletePermanently={() => selectedId && onDeletePermanently(selectedId)}
        onAddTag={() => {}}
        onRemoveTag={() => {}}
        onDelete={() => {}}
        onTogglePrintStatus={() => {}}
        onToggleFavorite={() => {}}
        onToggleQueue={() => {}}
        onUploadImage={() => {}}
        onSnapshotCaptured={() => {}}
        onSetSourceUrl={() => {}}
        onOpenInSlicer={() => {}}
        slicerError={null}
      />
    </div>
    </BundleCardsContext.Provider>
  );
}
