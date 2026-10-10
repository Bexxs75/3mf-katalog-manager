import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { RevealFileButton } from './RevealFileButton';
import { useImageUpload } from '../hooks/useImageUpload';
import { ImageUploadError } from './ImageUploadError';
import { Icon } from './Icon';
import { useImportLock } from '../hooks/ImportLockContext';
import { useEffect, useState } from 'react';
import { StatusBadges } from './StatusBadges';
import type { FilamentCheck } from '../types';
import type { LastPrinter } from '../lib/api/lastPrinter';
import { TagDot } from './TagDot';
import { TagInput } from './TagInput';
import type { ModelFile } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { ModelPreview } from './ModelPreview';
import { useUiDensity } from '../hooks/UiDensityContext';
import { buildMetaRows } from '../lib/modelMetadata';
import { isSafeHttpUrl } from '../lib/safeUrl';
import { formatDate } from '../i18n/format';
import { useEditableSourceUrl } from '../hooks/useEditableSourceUrl';
import { tagLabel } from '../lib/autoTags';
import type { AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';

interface Props {
  filament?: FilamentCheck | null;
  lastPrinter?: LastPrinter | null;
  allTags: string[];
  /** Color hue per tag label, so tag chips show the same dot as in the sidebar. */
  tagHues?: Record<string, number>;
  model: ModelFile | null;
  trashMode?: boolean;
  onRestore?: () => void;
  onDeletePermanently?: () => void;
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
  hasSlicer?: boolean;
  slicerError: AppError | null;
}

export function DetailPanel({
  filament, lastPrinter,
  model,
  allTags,
  tagHues,
  trashMode,
  onRestore,
  onDeletePermanently,
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
  hasSlicer,
  slicerError,
}: Props) {
  const { language } = useLanguage();
  const { lockProps } = useImportLock();
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const imageUpload = useImageUpload(model?.id, onUploadImage);
  const { density } = useUiDensity();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const {
    editing: editingSourceUrl,
    draft: sourceUrlDraft,
    setDraft: setSourceUrlDraft,
    startEditing: startEditingSourceUrl,
    handleKeyDown: handleSourceUrlKeyDown,
    handleBlur: handleSourceUrlBlur,
  } = useEditableSourceUrl(model, onSetSourceUrl);

  useEffect(() => {
    setConfirmDelete(false);
  }, [model?.id]);

  if (!model) {
    return (
      <aside role={trashMode ? undefined : "presentation"} className="flex-none w-[336px] flex items-center justify-center bg-[var(--panel)] border-l border-[var(--line)] text-[var(--ink-3)] text-body px-6 text-center">
        {t('emptyStateText')}
      </aside>
    );
  }

  if (trashMode) {
    const rows = buildMetaRows(model, t, language);
    const expiryDate = model.deletedAt
      ? new Date(new Date(model.deletedAt).getTime() + 7 * 24 * 60 * 60 * 1000)
      : null;
    return (
      <aside className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
        <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
          <div className="text-body font-semibold leading-tight break-words">{model.name}</div>
          <div className="font-code text-caption text-[var(--ink-3)] pt-1.5">{model.path}</div>
        </div>

        <div className="relative h-[248px] bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <ModelPreview model={model} needsSnapshot={false} onSnapshotCaptured={() => {}} />
        </div>

        <div className="flex-1 overflow-y-auto px-4 pt-3.5 pb-1">
          <div className="text-compact-meta ui-label text-[var(--ink-3)] pb-2">
            {t('metadataHeading')}
          </div>
          {rows.map((row) => (
            <div key={row.label} className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]">
              <span className="flex-none w-[108px] text-small text-[var(--ink-2)]">{row.label}</span>
              <span className="flex-1 font-medium tabular-nums text-small text-right">{row.value}</span>
            </div>
          ))}
        </div>

        <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)] flex flex-col gap-2">
          {expiryDate && (
            <p className="font-medium tabular-nums text-caption text-[var(--ink-3)]">
              {t('trashExpiryHint').replace('{date}', formatDate(expiryDate.toISOString(), language))}
            </p>
          )}
          <div className="flex gap-2">
            <button {...lockProps}
              onClick={onRestore}
              className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('restoreLabel')}
            </button>
            <button {...lockProps}
              onClick={onDeletePermanently}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-8 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer"
            >
              {t('deletePermanentlyLabel')}
            </button>
          </div>
        </div>
      </aside>
    );
  }

  if (density === 'compact') {
    return (
    <aside role={trashMode ? undefined : "presentation"} className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
      <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
        <div className="text-body font-semibold leading-tight break-words">{model.name}</div>
        <StatusBadges model={model} filament={filament} lastPrinter={lastPrinter} />
        <div className="font-code text-caption text-[var(--ink-3)] pt-1.5">{model.path}</div>
      </div>

      <div className="flex-1 overflow-y-auto">
        <div className="relative bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 11px)',
            }}
          />
          <ModelPreview surfaceClassName="h-[248px]"
            model={model}
            needsSnapshot={!(model.hasRenderSnapshot ?? !!model.renderSnapshotImage)}
            onOpenInSlicer={hasSlicer ? onOpenInSlicer : undefined}
            onRemoveFromCatalog={onRemoveFromCatalog}
            onSnapshotCaptured={onSnapshotCaptured}
          />
          <div className="absolute left-2.5 top-56 ui-label text-[var(--ink-3)] pointer-events-none">
            {t('dragToRotate')}
          </div>
          <button
            onClick={imageUpload.uploadImage}
            className="relative m-2 h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>
        <ImageUploadError error={imageUpload.error} onDismiss={imageUpload.dismiss} />

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-small font-medium">
            {model.printStatus === 'printed' ? t('printedBadge') : t('notPrintedLabel')}
          </span>
          <button
            onClick={onTogglePrintStatus}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.printStatus === 'printed' ? t('markAsNotPrinted') : t('markAsPrinted')}
          </button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-small font-medium">
            {model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
          </span>
          <button
            onClick={onToggleFavorite}
            aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            <Icon name="heart" size={14} fill={model.favorite  ? 'currentColor' : 'none'} />
          </button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-small font-medium">
            {model.queuePosition !== null ? t('inQueueLabel') : t('notInQueueLabel')}
          </span>
          <button
            onClick={onToggleQueue}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.queuePosition !== null ? t('removeFromQueue') : t('addToQueue')}
          </button>
        </div>

        <div className="px-4 pt-3.5 pb-1">
          <div className="text-compact-meta ui-label text-[var(--ink-3)] pb-2">
            {t('metadataHeading')}
          </div>
          {buildMetaRows(model, t, language).filter(row => !model.materials.length || row.label !== t('metaMaterial')).map((row) => (
            <div
              key={row.label}
              className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]"
            >
              <span className="flex-none w-[108px] text-small text-[var(--ink-2)]">
                {row.label}
              </span>
              <span className="flex-1 font-medium tabular-nums text-small text-right">{row.value}</span>
            </div>
          ))}
          <div className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]">
            <span className="flex-none w-[108px] text-small text-[var(--ink-2)]">
              {t('metaSourceUrl')}
            </span>
            <span className="flex-1 flex items-center justify-end gap-1.5 min-w-0 font-medium tabular-nums text-small">
              {editingSourceUrl ? (
                <input
                  value={sourceUrlDraft}
                  onChange={(e) => setSourceUrlDraft(e.target.value)}
                  onKeyDown={handleSourceUrlKeyDown}
                  onBlur={handleSourceUrlBlur}
                  autoFocus
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-medium tabular-nums text-small"
                />
              ) : isSafeHttpUrl(model.sourceUrl) ? (
                <a
                  href={model.sourceUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                >
                  {model.sourceUrl}
                </a>
              ) : model.sourceUrl ? (
                // Not an http(s) value: plain text, never a clickable link.
                <span className="flex-1 min-w-0 truncate text-right text-[var(--ink-2)]">
                  {model.sourceUrl}
                </span>
              ) : (
                <span className="flex-1 text-right text-[var(--ink-3)]">{t('noValue')}</span>
              )}
              <span
                onClick={startEditingSourceUrl}
                className="flex-none w-4 h-4 grid place-items-center rounded-full cursor-pointer text-compact-meta text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
              >
                <Icon name="edit" size={14} />
              </span>
            </span>
          </div>
        </div>

        <div className="px-4 pt-[18px] pb-5">
          <div className="text-compact-meta ui-label text-[var(--ink-3)] pb-2.5">
            {t('hashtagsHeading')}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {model.tags.map((tag) => (
              <span
                key={tag}
                className="inline-flex items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-medium tabular-nums text-caption"
              >
                <TagDot hue={tagHues?.[tag]} />
                #{tagLabel(tag, language)}
                <span
                  onClick={() => onRemoveTag(tag)}
                  className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-compact-meta hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                >
                  <Icon name="close" size={14} />
                </span>
              </span>
            ))}
            <TagInput
              key={model.id}
              allTags={allTags}
              tags={model.tags}
              onAddTag={onAddTag}
              stripHash
              inputClassName="h-6 w-[118px] px-2.5 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-medium tabular-nums text-caption"
            />
          </div>
        </div>
      </div>

      <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        {slicerError && (
          <div className="pb-2 font-medium tabular-nums text-compact-meta text-[var(--accent)] break-words">
            {t('slicerLaunchError')} <ErrorText error={slicerError} />
          </div>
        )}
        <div className="flex gap-2">
          {confirmDelete ? (
            <>
              <span className="flex-1 flex items-center text-small font-medium text-[var(--ink)]">
                {t('deleteConfirmQuestion')}
              </span>
              <button
                onClick={() => setConfirmDelete(false)}
                className="flex-none h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>
              <button {...lockProps}
                onClick={() => {
                  setConfirmDelete(false);
                  onDelete();
                }}
                className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-none h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer"
              >
                {t('delete')}
              </button>
            </>
          ) : (
            <>
              <RevealFileButton fileId={model.id} compact />
              {!container && (<button
                onClick={() => onOpenInSlicer()}
                className="flex-1 min-w-0 h-8 px-2 truncate rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('openInSlicer')}
              </button>)}
              <button {...lockProps}
                onClick={() => setConfirmDelete(true)}
                aria-label={t('deleteAriaLabel')}
                className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-medium tabular-nums cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                <Icon name="close" size={14} />
              </button>
            </>
          )}
        </div>
      </div>
    </aside>
    );
  }

  return (
    <aside role={trashMode ? undefined : "presentation"}
      className="flex-none flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]"
      style={{ width: '380px' }}
    >
      <div className="flex-1 overflow-y-auto">
        <div className="relative bg-[var(--plate)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 12px)',
            }}
          />
          <ModelPreview surfaceClassName="h-[284px]"
            model={model}
            needsSnapshot={!(model.hasRenderSnapshot ?? !!model.renderSnapshotImage)}
            onOpenInSlicer={hasSlicer ? onOpenInSlicer : undefined}
            onRemoveFromCatalog={onRemoveFromCatalog}
            onSnapshotCaptured={onSnapshotCaptured}
          />
          <button
            onClick={imageUpload.uploadImage}
            className="relative m-3 h-9 px-3.5 rounded-lg bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--ink-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
            style={{ fontSize: 'var(--font-size-meta)' }}
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>
        <ImageUploadError error={imageUpload.error} onDismiss={imageUpload.dismiss} />

        <div className="px-[18px] pt-4 pb-1 flex items-start justify-between gap-3">
          <div className="font-semibold leading-tight break-words" style={{ fontSize: 'var(--font-size-title)' }}>
            {model.name}
          </div>
          <button
            onClick={onToggleFavorite}
            aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
            className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-none text-heading ${model.favorite ? 'text-[var(--accent)]' : 'text-[var(--ink-3)]'}`}
          >
            <Icon name="heart" size={14} fill={model.favorite  ? 'currentColor' : 'none'} />
          </button>
        </div>
        <div className="px-[18px]"><StatusBadges model={model} filament={filament} lastPrinter={lastPrinter} /></div>
        <div className="px-[18px] pb-3.5 flex flex-wrap gap-1.5">
          {model.tags.map((tag) => (
            <span
              key={tag}
              className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-meta)' }}
            >
              <TagDot hue={tagHues?.[tag]} />
              #{tagLabel(tag, language)}
              <span
                onClick={() => onRemoveTag(tag)}
                className="w-4 h-4 grid place-items-center rounded-full cursor-pointer hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                style={{ fontSize: 'var(--font-size-label)' }}
              >
                <Icon name="close" size={14} />
              </span>
            </span>
          ))}
          <TagInput
            key={model.id}
            allTags={allTags}
            tags={model.tags}
            onAddTag={onAddTag}
            stripHash
            inputClassName="px-2.5 py-1 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0"
            inputStyle={{ fontSize: 'var(--font-size-meta)' }}
          />
        </div>

        <div className="px-[18px] pb-4 border-t border-[var(--line)] pt-3.5">
          <div
            className="ui-label text-[var(--ink-3)] mb-2.5"
            style={{ fontSize: 'var(--font-size-label)' }}
          >
            {t('metadataHeading')}
          </div>
          {buildMetaRows(model, t, language).filter(row => !model.materials.length || row.label !== t('metaMaterial')).map((row) => (
            <div
              key={row.label}
              className="flex justify-between py-2 border-b border-[var(--line)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              <span className="text-[var(--ink-2)]">{row.label}</span>
              <span className="font-semibold">{row.value}</span>
            </div>
          ))}
          <div className="flex justify-between items-center gap-2 py-2" style={{ fontSize: 'var(--font-size-body)' }}>
            <span className="text-[var(--ink-2)]">{t('metaSourceUrl')}</span>
            {editingSourceUrl ? (
              <input
                value={sourceUrlDraft}
                onChange={(e) => setSourceUrlDraft(e.target.value)}
                onKeyDown={handleSourceUrlKeyDown}
                onBlur={handleSourceUrlBlur}
                autoFocus
                placeholder={t('sourceUrlPlaceholder')}
                className="flex-1 min-w-0 px-2 py-1 rounded-md border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0"
              />
            ) : (
              <span className="flex-1 flex items-center justify-end gap-1.5 min-w-0">
                {isSafeHttpUrl(model.sourceUrl) ? (
                  <a
                    href={model.sourceUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                  >
                    {model.sourceUrl}
                  </a>
                ) : model.sourceUrl ? (
                  // Not an http(s) value: plain text, never a clickable link.
                  <span className="flex-1 min-w-0 truncate text-right text-[var(--ink-2)]">
                    {model.sourceUrl}
                  </span>
                ) : (
                  <span className="flex-1 text-right text-[var(--ink-3)]">{t('noValue')}</span>
                )}
                <span
                  onClick={startEditingSourceUrl}
                  className="flex-none w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
                  style={{ fontSize: 'var(--font-size-label)' }}
                >
                  <Icon name="edit" size={14} />
                </span>
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="flex-none px-[18px] py-4 border-t border-[var(--line)] bg-[var(--panel-2)] flex flex-col gap-2">
        {slicerError && (
          <div className="text-[var(--accent)] break-words" style={{ fontSize: 'var(--font-size-meta)' }}>
            {t('slicerLaunchError')} <ErrorText error={slicerError} />
          </div>
        )}
        {confirmDelete ? (
          <div className="flex items-center gap-2">
            <span className="flex-1 font-medium" style={{ fontSize: 'var(--font-size-body)' }}>
              {t('deleteConfirmQuestion')}
            </span>
            <button
              onClick={() => setConfirmDelete(false)}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-10 px-4 rounded-lg border border-[var(--line-strong)] bg-[var(--panel)] font-semibold cursor-pointer"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {t('cancel')}
            </button>
            <button {...lockProps}
              onClick={() => {
                setConfirmDelete(false);
                onDelete();
              }}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-10 px-4 rounded-lg bg-[var(--accent)] text-[var(--accent-ink)] font-semibold cursor-pointer"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {t('delete')}
            </button>
          </div>
        ) : (
          <>
            <RevealFileButton fileId={model.id} compact />
            {!container && (<button
              onClick={() => onOpenInSlicer()}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-11 px-4 flex items-center gap-2.5 justify-center font-bold cursor-pointer bg-[var(--accent)] text-[var(--accent-ink)] rounded-lg"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              🖨 {t('openInSlicer')}
            </button>)}
            <button
              onClick={onToggleQueue}
              className="h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              🎞 {model.queuePosition !== null ? t('removeFromQueue') : t('addToQueue')}
            </button>
            <button
              onClick={onTogglePrintStatus}
              className="h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {model.printStatus === 'printed' && <Icon name="check" size={14} />}{t(model.printStatus === 'printed' ? 'markAsNotPrinted' : 'markAsPrinted')}
            </button>
            <button {...lockProps}
              onClick={() => setConfirmDelete(true)}
              aria-label={t('deleteAriaLabel')}
              className="h-9 rounded-lg text-[var(--ink-3)] hover:text-[var(--accent)] cursor-pointer"
              style={{ fontSize: 'var(--font-size-meta)' }}
            >
              {t('deleteAriaLabel')}
            </button>
          </>
        )}
      </div>
    </aside>
  );
}
