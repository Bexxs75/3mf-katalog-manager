import { RevealFileButton } from './RevealFileButton';
import { useImageUpload } from '../hooks/useImageUpload';
import { ImageUploadError } from './ImageUploadError';
import { useModelImages } from '../hooks/useModelImages';
import { useImportLock } from '../hooks/ImportLockContext';
import { useRef, useState } from 'react';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { Icon } from './Icon';
import { useDetailNavigation, type DetailDirection } from '../hooks/useDetailNavigation';
import { TagDot } from './TagDot';
import { TagInput } from './TagInput';
import { invoke } from '@tauri-apps/api/core';
import type { ModelFile, SlicerConfig, Collection } from '../types';
import { useT, useLanguage } from '../i18n/LanguageContext';
import { buildMetaRows } from '../lib/modelMetadata';
import { isSafeHttpUrl } from '../lib/safeUrl';
import { ModelPreview } from './ModelPreview';
import { resolveDisplayImage } from '../lib/resolveDisplayImage';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import { useEditableSourceUrl } from '../hooks/useEditableSourceUrl';
import { usePrintLog } from '../hooks/usePrintLog';
import { useFilamentCheck } from '../hooks/useFilamentCheck';
import { formatWeightG, formatLengthM, formatPrice } from '../i18n/format';
import { tagLabel } from '../lib/autoTags';
import { FilamentCheckSection } from './FilamentCheckSection';
import type { AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';

interface Props {
  allTags: string[];
  /** Color hue per tag label, so tag chips show the same dot as in the sidebar. */
  tagHues?: Record<string, number>;
  model: ModelFile;
  onClose: () => void;
  onNavigate?: (direction: DetailDirection) => void;
  hasPrevious: boolean;
  hasNext: boolean;
  position?: { index: number; total: number };
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onTogglePrintStatus: () => void;
  onToggleFavorite: () => void;
  onToggleQueue: () => void;
  onUploadImage: () => void | Promise<void>;
  onSnapshotCaptured: (base64: string) => void;
  onSetSourceUrl: (fileId: string, url: string | null) => void;
  onOpenInSlicer: () => void;
  onRemoveFromCatalog?: () => Promise<void>;
  onRescanMetadata: () => void;
  onAddToCollection: (collectionId: string) => void;
  collections: Collection[];
  slicers: SlicerConfig[];
  slicerError: AppError | null;
  rescanError: AppError | null;
  rescanSuccess: boolean;
  displayPreference: DisplayPreference;
}

export function ModelDetailPage({
  model,
  allTags,
  tagHues,
  onClose,
  onNavigate,
  hasPrevious,
  hasNext,
  position,
  onAddTag,
  onRemoveTag,
  onDelete,
  onTogglePrintStatus,
  onToggleFavorite,
  onToggleQueue,
  onUploadImage,
  onSnapshotCaptured,
  onSetSourceUrl,
  onOpenInSlicer,
  onRemoveFromCatalog,
  onRescanMetadata,
  onAddToCollection,
  collections,
  slicers,
  slicerError,
  rescanError,
  rescanSuccess,
  displayPreference,
}: Props) {
  const [pendingNavigation, setPendingNavigation] = useState<DetailDirection | null>(null);
  const navigationTrigger = useRef<HTMLButtonElement | null>(null);
  const pageRef = useRef<HTMLDivElement>(null);
  const { lockProps } = useImportLock();
  const t = useT();
  const imageUpload = useImageUpload(model?.id, onUploadImage);
  const { language } = useLanguage();
  const [addToCollectionMenuOpen, setAddToCollectionMenuOpen] = useState(false);
  const {
    editing: editingSource,
    draft: sourceDraft,
    setDraft: setSourceDraft,
    startEditing: startEditingSource,
    handleKeyDown: handleSourceKeyDown,
    handleBlur: handleSourceBlur,
  } = useEditableSourceUrl(model, onSetSourceUrl);
  const images = useModelImages([model.id]);
  const resolvedImage = resolveDisplayImage(model, displayPreference, images.get(model.id));
  const [showCustomImage, setShowCustomImage] = useState(
    () => displayPreference === 'thumbnail',
  );

  const rows = buildMetaRows(model, t, language);
  const filamentCheck = useFilamentCheck(
    model.sliceInfo ? [model.id] : [],
    JSON.stringify(model.sliceInfo ?? null),
  );

  const {
    entries: printLogEntries,
    error: printLogError,
    addEntry: addPrintLogEntry,
    deleteEntry: deletePrintLogEntry,
  } = usePrintLog(model.id);
  const [showPrintLogForm, setShowPrintLogForm] = useState(false);
  const [printLogDate, setPrintLogDate] = useState(() => new Date().toISOString().slice(0, 10));
  const [printLogNote, setPrintLogNote] = useState('');
  const [printLogPhoto, setPrintLogPhoto] = useState<string | null>(null);

  const isEditing = () => editingSource || showPrintLogForm ||
    Array.from(pageRef.current?.querySelectorAll<HTMLInputElement>('input') ?? [])
      .some(input => input.value.trim() !== '');
  const navigate = (direction: DetailDirection) => {
    if ((direction === 'previous' ? hasPrevious : hasNext)) onNavigate?.(direction);
  };
  useDetailNavigation(onNavigate ? navigate : undefined, isEditing);
  const navigateByButton = (direction: DetailDirection, trigger: HTMLButtonElement) => {
    if (isEditing()) {
      navigationTrigger.current = trigger;
      setPendingNavigation(direction);
    } else navigate(direction);
  };
  const navigationButtonClass = 'relative flex-none w-[42px] h-[42px] rounded-[10px] border border-transparent grid place-items-center cursor-pointer text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] disabled:opacity-40 disabled:cursor-default';

  const submitPrintLogEntry = () => {
    addPrintLogEntry(new Date(printLogDate).toISOString(), printLogNote.trim() || null, printLogPhoto)
      .then(() => {
        setShowPrintLogForm(false);
        setPrintLogNote('');
        setPrintLogPhoto(null);
        setPrintLogDate(new Date().toISOString().slice(0, 10));
      })
      .catch((e) => console.error('[print-log] adding failed:', e));
  };

  const pickPrintLogPhoto = () => {
    invoke<string | null>('pick_and_read_image').then((base64) => {
      if (base64) setPrintLogPhoto(base64);
    });
  };

  return (
    <div ref={pageRef} className="flex-1 min-w-0 overflow-y-auto [scrollbar-gutter:stable] p-6 flex flex-col gap-6 max-w-[1600px] w-full mx-auto">
      {pendingNavigation !== null && <CatalogActionDialog title={t('detailDiscardEdits')} returnFocus={navigationTrigger} onClose={() => setPendingNavigation(null)}>
        <div className="flex justify-end gap-2">
          <button data-initial-focus className={catalogActionButton} onClick={() => setPendingNavigation(null)}>{t('detailKeepEditing')}</button>
          <button className={catalogActionButton} onClick={() => { const direction = pendingNavigation; setPendingNavigation(null); navigate(direction); }}>{t('detailDiscardAndGo')}</button>
        </div>
      </CatalogActionDialog>}
      <header className="flex items-start gap-4">
        <button
          onClick={onClose}
          title={t('backToCatalog')}
          className="flex-none w-11 h-11 rounded-md border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] grid place-items-center hover:border-[var(--line-strong)] hover:text-[var(--ink)]"
        >
          <svg viewBox="0 0 24 24" className="w-5 h-5" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M19 12H5M11 18l-6-6 6-6" />
          </svg>
        </button>
        <button className={navigationButtonClass} aria-label={t('detailPrevious')} title={t('detailPrevious')}
          disabled={!hasPrevious || !onNavigate} onMouseDown={event => event.preventDefault()}
          onClick={event => navigateByButton('previous', event.currentTarget)}><Icon name="previous" /></button>
        {position && <span className="font-medium tabular-nums text-small text-[var(--ink-3)] self-center whitespace-nowrap">
          {t('detailPosition').replace('{index}', String(position.index)).replace('{total}', String(position.total))}
        </span>}
        <button className={navigationButtonClass} aria-label={t('detailNext')} title={t('detailNext')}
          disabled={!hasNext || !onNavigate} onMouseDown={event => event.preventDefault()}
          onClick={event => navigateByButton('next', event.currentTarget)}><Icon name="next" /></button>
        <h1 className="flex-1 min-w-0 text-[1.5rem] font-semibold leading-tight break-words">
          {model.name}
        </h1>
      </header>

      <div className="flex gap-7 flex-wrap items-start">
        <div className="flex-1 min-w-[320px]">
          <div data-detail-viewer tabIndex={0}
            onPointerDown={event => {
              if (event.target === event.currentTarget || event.target instanceof HTMLCanvasElement) event.currentTarget.focus();
            }}
            className="relative aspect-[4/3] rounded-[10px] border border-[var(--line)] bg-[var(--plate)] overflow-hidden">
            {showCustomImage && resolvedImage ? (
              <img src={resolvedImage} alt={model.name} className="absolute inset-0 w-full h-full object-contain" />
            ) : (
              <ModelPreview
                model={model}
                needsSnapshot={!(model.hasRenderSnapshot ?? !!model.renderSnapshotImage)}
                onSnapshotCaptured={onSnapshotCaptured}
                onOpenInSlicer={slicers.length ? onOpenInSlicer : undefined}
                onRemoveFromCatalog={onRemoveFromCatalog}
                showRotationControls
              />
            )}
            {resolvedImage && (
              <div className="absolute top-3 right-3 flex bg-[var(--panel-2)] border border-[var(--line)] rounded-full overflow-hidden font-medium tabular-nums text-caption">
                <button
                  onClick={() => setShowCustomImage(false)}
                  className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] px-3.5 py-1.5 ${!showCustomImage ? 'bg-[var(--accent)] text-[var(--accent-ink)] font-semibold' : 'text-[var(--ink-3)]'}`}
                >
                  {t('detailViewer3d')}
                </button>
                <button
                  onClick={() => setShowCustomImage(true)}
                  className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] px-3.5 py-1.5 ${showCustomImage ? 'bg-[var(--accent)] text-[var(--accent-ink)] font-semibold' : 'text-[var(--ink-3)]'}`}
                >
                  {t('detailViewerImage')}
                </button>
              </div>
            )}
          </div>
          <button
            onClick={imageUpload.uploadImage}
            className="mt-2 h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-small font-semibold hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('uploadModelImageLabel')}
          </button>
          <ImageUploadError error={imageUpload.error} onDismiss={imageUpload.dismiss} />
        </div>

        <div className="w-[540px] flex-none min-w-[300px] flex flex-col gap-4">
          <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
            {rows.map((row) => (
              <div
                key={row.label}
                className="flex justify-between items-baseline gap-4 py-2 border-b border-[var(--line)] last:border-b-0 text-body"
              >
                <span className="text-[var(--ink-3)]">{row.label}</span>
                <span className="font-medium tabular-nums tabular-nums text-right">{row.value}</span>
              </div>
            ))}
            <div className="flex justify-between items-baseline gap-4 py-2 border-b border-[var(--line)] text-body">
              <span className="text-[var(--ink-3)]">{t('metaCreator')}</span>
              <span className="font-medium tabular-nums text-right">{model.creator ?? t('noValue')}</span>
            </div>
            <div className="flex justify-between items-baseline gap-4 py-2 text-body">
              <span className="text-[var(--ink-3)]">{t('metaSourceUrl')}</span>
              {editingSource ? (
                <input
                  autoFocus
                  value={sourceDraft}
                  onChange={(e) => setSourceDraft(e.target.value)}
                  onBlur={() => { if (pendingNavigation === null) handleSourceBlur(); }}
                  onKeyDown={handleSourceKeyDown}
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-body"
                />
              ) : model.sourceUrl ? (
                <span className="text-right">
                  {isSafeHttpUrl(model.sourceUrl) ? (
                    <a href={model.sourceUrl} target="_blank" rel="noreferrer" className="underline decoration-[var(--line-strong)] underline-offset-2">
                      {model.sourceUrl}
                    </a>
                  ) : (
                    // Not an http(s) value: plain text, never a clickable link.
                    <span className="text-[var(--ink-2)]">{model.sourceUrl}</span>
                  )}{' '}
                  <button aria-label={t('pmEdit')} onClick={startEditingSource} className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] text-[var(--ink-3)]"><Icon name="edit" size={14} /></button>
                </span>
              ) : (
                <button onClick={startEditingSource} className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] text-[var(--ink-3)] underline decoration-dotted">
                  {t('sourceUrlPlaceholder')} <Icon name="edit" size={14} />
                </button>
              )}
            </div>

            <div className="mt-4">
              <p className="ui-label text-[var(--ink-3)] mb-2">
                {t('hashtagsHeading')}
              </p>
              <div className="flex flex-wrap gap-1.5">
                {model.tags.map((tag) => (
                  <span key={tag} className="inline-flex items-center gap-1.5 font-medium tabular-nums text-small px-2.5 py-1 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]">
                    <TagDot hue={tagHues?.[tag]} />
                    #{tagLabel(tag, language)}{' '}
                    <button aria-label={t('chipRemove').replace('{label}', tag)} onClick={() => onRemoveTag(tag)} className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] text-[var(--ink-3)]"><Icon name="close" size={14} /></button>
                  </span>
                ))}
                <TagInput
                  key={model.id}
                  allTags={allTags}
                  tags={model.tags}
                  onAddTag={onAddTag}
                  onEscape={onClose}
                  inputClassName="font-medium tabular-nums text-small px-2.5 py-1 rounded-full bg-transparent border border-dashed border-[var(--line)] text-[var(--ink-3)] w-32"
                />
              </div>
            </div>
          </div>

          <div className="flex flex-wrap gap-2 items-center">
            <button
              onClick={onTogglePrintStatus}
              className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-small font-semibold ${
                model.printStatus === 'printed'
                  ? 'bg-[var(--good-soft,var(--accent-soft))] text-[var(--good,var(--accent))]'
                  : 'bg-[var(--panel-2)] text-[var(--ink-2)] border border-[var(--line)]'
              }`}
            >
              {model.printStatus === 'printed' ? t('printedBadge') : t('notPrintedLabel')}
            </button>
            <button
              onClick={onToggleFavorite}
              aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              title={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] w-9 h-9 rounded-md border grid place-items-center ${
                model.favorite ? 'border-[var(--accent)] text-[var(--accent)] bg-[var(--accent-soft)]' : 'border-[var(--line)] text-[var(--ink-2)]'
              }`}
            >
              <Icon name="heart" size={14} fill={model.favorite ? 'currentColor' : 'none'} />
            </button>
            <button onClick={onToggleQueue} className="text-body font-semibold text-[var(--ink-2)] hover:text-[var(--ink)]">
              {model.queuePosition === null && <Icon name="plus" size={14} />}{t(model.queuePosition !== null ? 'removeFromQueue' : 'addToQueue')}
            </button>
          </div>
        </div>
      </div>

      {model.sliceInfo && (
        <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
          <p className="ui-label text-[var(--ink-3)] mb-3">
            {t('sliceFilamentHeading')}
          </p>
          <div className="flex flex-col gap-3">
            {model.sliceInfo.plates.map((plate) => (
              <div key={plate.plateIndex}>
                <p className="text-small font-semibold text-[var(--ink-2)] mb-1.5">
                  {t('sliceFilamentPlateLabel').replace('{index}', String(plate.plateIndex))}
                </p>
                <div className="flex flex-col gap-1">
                  {plate.filaments.map((filament, i) => (
                    <div key={i} className="flex items-center gap-2 text-body">
                      {filament.color && (
                        <span
                          className="w-3 h-3 rounded-full border border-[var(--line)] flex-none"
                          style={{ backgroundColor: filament.color.slice(0, 7) }}
                        />
                      )}
                      <span className="text-[var(--ink-2)]">{filament.type}</span>
                      <span className="font-medium tabular-nums tabular-nums text-[var(--ink-3)]">
                        {formatWeightG(filament.usedG, language)} · {formatLengthM(filament.usedM, language)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
          <FilamentCheckSection
            check={filamentCheck.checks?.get(model.id) ?? null}
            error={filamentCheck.error}
          />
          {model.costEstimate && (
            <div className="flex items-center justify-between gap-2 mt-3 pt-3 border-t border-[var(--line)] text-body">
              <span className="text-[var(--ink-3)]">
                {t('metaCostEstimate')}
                {model.costEstimate.hasUnpricedFilaments && (
                  <span title={t('costEstimateUnpricedHint')} className="ml-1 text-[var(--ink-3)]">
                    *
                  </span>
                )}
              </span>
              <span className="font-medium tabular-nums tabular-nums">
                {model.costEstimate.totalCost === null
                  ? t('noValue')
                  : formatPrice(model.costEstimate.totalCost, language)}
              </span>
            </div>
          )}
        </div>
      )}

      <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
        <div className="flex items-center justify-between mb-3">
          <p className="ui-label text-[var(--ink-3)]">
            {t('printLogHeading')}
          </p>
          {!showPrintLogForm && (
            <button
              onClick={() => setShowPrintLogForm(true)}
              className="text-small font-semibold text-[var(--accent)] hover:underline"
             aria-label={t('printLogAddButton')}>
              <Icon name="plus" size={14} /> {t('printLogAddButton').replace(/^\+\s*/, '')}
            </button>
          )}
        </div>

        {showPrintLogForm && (
          <div className="flex flex-col gap-2 mb-4 p-3 rounded-[6px] border border-dashed border-[var(--line-strong)]">
            <input
              type="date"
              value={printLogDate}
              onChange={(e) => setPrintLogDate(e.target.value)}
              className="bg-[var(--panel-2)] border border-[var(--line)] rounded px-2 py-1 text-body"
            />
            <textarea
              value={printLogNote}
              onChange={(e) => setPrintLogNote(e.target.value)}
              placeholder={t('printLogNotePlaceholder')}
              rows={2}
              className="bg-[var(--panel-2)] border border-[var(--line)] rounded px-2 py-1 text-body resize-none"
            />
            <div className="flex items-center gap-2">
              <button
                onClick={pickPrintLogPhoto}
                className="h-7 px-3 rounded-[3px] border border-[var(--line)] bg-transparent text-[var(--ink-2)] text-small hover:border-[var(--accent)]"
              >
                {t('printLogPhotoButton')}
              </button>
              {printLogPhoto && (
                <img
                  src={`data:image/png;base64,${printLogPhoto}`}
                  alt=""
                  className="w-8 h-8 rounded object-cover border border-[var(--line)]"
                />
              )}
            </div>
            <div className="flex gap-2 justify-end">
              <button
                onClick={() => {
                  setShowPrintLogForm(false);
                  setPrintLogNote('');
                  setPrintLogPhoto(null);
                }}
                className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-7 px-3 rounded-[3px] border border-[var(--line)] bg-transparent text-[var(--ink-2)] text-small"
              >
                {t('printLogCancelButton')}
              </button>
              <button
                onClick={submitPrintLogEntry}
                className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-7 px-3 rounded-[3px] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold"
              >
                {t('printLogSaveButton')}
              </button>
            </div>
          </div>
        )}

        {printLogEntries.length === 0 ? (
          <p className="text-body text-[var(--ink-3)]">{t('printLogEmpty')}</p>
        ) : (
          <div className="flex flex-col gap-2.5">
            {printLogEntries.map((entry) => (
              <div key={entry.id} className="flex items-start gap-2.5 text-body">
                {entry.photoImage && (
                  <img
                    src={entry.photoImage}
                    alt=""
                    className="w-10 h-10 rounded object-cover border border-[var(--line)] flex-none"
                  />
                )}
                <div className="flex-1 min-w-0">
                  <div className="font-medium tabular-nums text-[var(--ink-3)]">
                    {new Date(entry.printedAt).toLocaleDateString(language, { timeZone: 'UTC' })}
                  </div>
                  {entry.note && <div className="text-[var(--ink-2)]">{entry.note}</div>}
                </div>
                <button
                  onClick={() =>
                    deletePrintLogEntry(entry.id).catch((e) =>
                      console.error('[print-log] deleting failed:', e),
                    )
                  }
                  className="text-[var(--ink-3)] hover:text-red-400 flex-none"
                >
                  <Icon name="close" size={14} />
                </button>
              </div>
            ))}
          </div>
        )}
        {printLogError && <p className="text-small text-red-400"><ErrorText error={printLogError} /></p>}
      </div>

      <footer className="flex items-center justify-between gap-4 flex-wrap rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-4 py-3.5 mt-auto">
        <span className="font-code text-small text-[var(--ink-3)] break-all">{model.path}</span>
        <div className="flex gap-2.5 items-center flex-none">
          <button {...lockProps} onClick={onDelete} className="px-4 py-2 rounded-md border border-[var(--line)] text-body font-semibold text-red-400 hover:border-red-400">
            {t('delete')}
          </button>
          <button
            onClick={onRescanMetadata}
            className="px-4 py-2 rounded-md border border-[var(--line)] text-body font-semibold text-[var(--ink-2)] hover:border-[var(--line-strong)]"
          >
            {t('rescanMetadataButton')}
          </button>
          <div className="relative">
            <button
              onClick={() => setAddToCollectionMenuOpen((prev) => !prev)}
              className="px-4 py-2 rounded-md border border-[var(--line)] text-body font-semibold text-[var(--ink-2)] hover:border-[var(--line-strong)]"
            >
              {t('addToCollectionLabel')}
            </button>
            {addToCollectionMenuOpen && (
              <div data-navigation-menu className="absolute bottom-11 right-0 min-w-[220px] max-w-[360px] py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
                {collections.map((c) => (
                  <button
                    key={c.id}
                    title={c.name}
                    onClick={() => {
                      onAddToCollection(c.id);
                      setAddToCollectionMenuOpen(false);
                    }}
                    className="w-full text-left px-3 py-1.5 text-body text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer whitespace-nowrap overflow-hidden text-ellipsis"
                  >
                    {c.name}
                  </button>
                ))}
                {collections.length === 0 && (
                  <div className="px-3 py-1.5 font-medium tabular-nums text-caption text-[var(--ink-3)]">
                    {t('noCollectionsEmptyState')}
                  </div>
                )}
              </div>
            )}
          </div>
          <RevealFileButton fileId={model.id} />
          <button
            onClick={() => onOpenInSlicer()}
            disabled={slicers.length === 0}
            className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] px-4 py-2 rounded-md bg-[var(--accent)] text-[var(--accent-ink)] text-body font-semibold disabled:opacity-50"
          >
            {t('openInSlicer')} <Icon name="external" size={14} />
          </button>
        </div>
      </footer>
      {slicerError && <p className="text-small text-red-400"><ErrorText error={slicerError} /></p>}
      {rescanError && <p className="text-small text-red-400"><ErrorText error={rescanError} /></p>}
      {rescanSuccess && (
        <p className="text-small text-[var(--good,var(--accent))]">{t('rescanMetadataSuccess')}</p>
      )}
    </div>
  );
}
