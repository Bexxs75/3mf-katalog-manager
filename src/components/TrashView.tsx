import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { DetailPanel } from './DetailPanel';
import { useT } from '../i18n/LanguageContext';
import type { ModelFile, ViewMode } from '../types';
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
}: TrashViewProps) {
  const t = useT();
  // The trash doesn't support folder grouping (deleted files no longer
  // "belong" to an active folder context) - falls back to the respective
  // flat display if the shared toggle is set to a grouped option
  // while the trash view is shown.
  const effectiveIsGrid = view === 'grid' || view === 'groupedGrid';
  return (
    <div className="flex flex-1 min-h-0">
      <main className="flex-1 min-w-0 flex flex-col">
        <div className="flex-none flex items-center justify-between px-4 py-3 border-b border-[var(--line)]">
          <h1 className="text-[15px] font-semibold">{t('trashHeading')}</h1>
          {confirmEmptyTrash ? (
            <div className="flex items-center gap-2">
              <span className="text-[12.5px] font-medium text-[var(--ink)]">
                {t('emptyTrashConfirmQuestion')}
              </span>
              <button
                onClick={() => onConfirmEmptyTrashChange(false)}
                className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>
              <button
                onClick={onEmptyTrash}
                className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
              >
                {t('emptyTrashButton')}
              </button>
            </div>
          ) : (
            <button
              onClick={() => onConfirmEmptyTrashChange(true)}
              disabled={trashModels.length === 0}
              className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer disabled:opacity-50 hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('emptyTrashButton')}
            </button>
          )}
        </div>
        <div className="flex-1 overflow-y-auto overscroll-contain p-4">
          {trashModels.length === 0 ? (
            <p className="font-mono-ui text-[12.5px] text-[var(--ink-3)]">{t('trashEmptyState')}</p>
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
        model={trashModels.find((m) => m.id === selectedId) ?? null}
        trashMode
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
  );
}
