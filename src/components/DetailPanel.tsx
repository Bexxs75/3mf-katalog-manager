import { WebFileActions } from './WebFileActions';
import { WebRemoveFromCatalogButton } from './WebRemoveFromCatalogButton';
import { useCanEditMetadata } from '../lib/webSession';
import { isReadOnly } from '../lib/readOnly';
import { TRASH_RETENTION_DAYS } from '../lib/trashRetention';
import { displayName } from '../lib/displayName';
import { LicenseEditor } from './LicenseEditor';
import { CatalogWebButton } from './CatalogWebButton';
import { ExternalLink } from './ExternalLink';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { RevealFileButton } from './RevealFileButton';
import { WebSlicerMenu } from './WebSlicerMenu';
import { useImageUpload } from '../hooks/useImageUpload';
import { ImageUploadError } from './ImageUploadError';
import { ImageUploadButton } from './ImageUploadButton';
import { Icon } from './Icon';
import { CompanionSection } from './CompanionSection';
import { BuildPlateFit } from './BuildPlateFit';
import { useImportLock } from '../hooks/ImportLockContext';
import { useEffect, useState } from 'react';
import { StatusBadges } from './StatusBadges';
import type { FilamentCheck, Printer } from '../types';
import type { LastPrinter } from '../lib/api/lastPrinter';
import { TagDot } from './TagDot';
import { TagInput } from './TagInput';
import { TagRemoveButton } from './TagRemoveButton';
import { AutoTagBlockStatus } from './AutoTagBlockStatus';
import { useAutoTagBlock } from '../hooks/useAutoTagBlock';
import type { ModelFile } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { ModelPreview } from './ModelPreview';
import { useUiDensity } from '../hooks/UiDensityContext';
import { buildMetaRows } from '../lib/modelMetadata';
import { SourcePlatformRows } from './SourcePlatformRows';
import { isSafeHttpUrl } from '../lib/safeUrl';
import { formatDate } from '../i18n/format';
import { useEditableSourceUrl } from '../hooks/useEditableSourceUrl';
import { shortTagLabel, tagLabel } from '../lib/autoTags';
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
  /** Printers of the Printer Manager, for the "print bed" section (those with a build volume). */
  printers?: Printer[];
  onRestore?: () => void;
  onDeletePermanently?: () => void;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onTogglePrintStatus: () => void;
  onToggleFavorite: () => void;
  onToggleQueue: () => void;
  onUploadImage: (file?: File) => void | Promise<void>;
  onSnapshotCaptured: (base64: string) => void;
  onSetLicense?: (ids: string[], license: string | null) => Promise<void>;
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
  printers = [],
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
  onSetLicense,
  onOpenInSlicer,
  onRemoveFromCatalog,
  hasSlicer,
  slicerError,
}: Props) {
  const canEdit = useCanEditMetadata();
  const { language } = useLanguage();
  const { lockProps } = useImportLock();
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const imageUpload = useImageUpload(model?.id, onUploadImage);
  const { density } = useUiDensity();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const autoTagBlock = useAutoTagBlock(trashMode || !canEdit ? null : model, onRemoveTag, onAddTag);
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
      ? new Date(new Date(model.deletedAt).getTime() + TRASH_RETENTION_DAYS * 24 * 60 * 60 * 1000)
      : null;
    return (
      <aside className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
        <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
          <div className="text-body font-semibold leading-tight break-words">{displayName(model.name)}</div>
          <div className="font-code text-caption text-[var(--ink-3)] pt-1.5">{displayName(model.path)}</div>
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
          {expiryDate && import.meta.env.VITE_TARGET !== 'web' && (
            <p className="font-medium tabular-nums text-caption text-[var(--ink-3)]">
              {t('trashExpiryHint').replace('{date}', formatDate(expiryDate.toISOString(), language))}
            </p>
          )}
          <div className="flex gap-2">
            <WebFileActions key={model.id} fileIds={[model.id]} name={model.name} trash />
            <>{!isReadOnly() && <button {...lockProps}
              onClick={onRestore}
              className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('restoreLabel')}
            </button>}</>
            <>{!isReadOnly() && <button {...lockProps}
              onClick={onDeletePermanently}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-8 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer"
            >
              {t('deletePermanentlyLabel')}
            </button>}</>
          </div>
        </div>
      </aside>
    );
  }

  if (density === 'compact') {
    return (
    <aside role={trashMode ? undefined : "presentation"} className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
      <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
        <div className="text-body font-semibold leading-tight break-words">{displayName(model.name)}</div>
        <StatusBadges model={model} filament={filament} lastPrinter={lastPrinter} />
        <div className="font-code text-caption text-[var(--ink-3)] pt-1.5">{displayName(model.path)}</div>
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
            needsSnapshot={import.meta.env.VITE_TARGET !== 'web' && !(model.hasRenderSnapshot ?? !!model.renderSnapshotImage)}
            onOpenInSlicer={hasSlicer ? onOpenInSlicer : undefined}
            onRemoveFromCatalog={onRemoveFromCatalog}
            onSnapshotCaptured={onSnapshotCaptured}
          />
          <div className="absolute left-2.5 top-56 ui-label text-[var(--ink-3)] pointer-events-none">
            {t('dragToRotate')}
          </div>
          <ImageUploadButton onUpload={imageUpload.uploadImage} className="relative m-2 h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]" />
        </div>
        <ImageUploadError error={imageUpload.error} onDismiss={imageUpload.dismiss} />

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-small font-medium">
            {model.printStatus === 'printed' ? t('printedBadge') : t('notPrintedLabel')}
          </span>
          <button
            onClick={onTogglePrintStatus}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
           {...(!canEdit ? { disabled: true, title: t('webReadOnly') } : {})}>
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
           {...(!canEdit ? { disabled: true, title: t('webReadOnly') } : {})}>
            <Icon name="heart" size={14} fill={model.favorite  ? 'currentColor' : 'none'} />
          </button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-small font-medium">
            {model.queuePosition !== null ? t('inQueueLabel') : t('notInQueueLabel')}
          </span>
          <>{canEdit && <button
            onClick={onToggleQueue}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.queuePosition !== null ? t('removeFromQueue') : t('addToQueue')}
          </button>}</>
        </div>

        <div className="px-4 pt-3.5 pb-1">
          <div className="text-compact-meta ui-label text-[var(--ink-3)] pb-2">
            {t('metadataHeading')}
          </div>
          <LicenseEditor key={model.id} value={model.license} onSave={onSetLicense ? value => onSetLicense([model.id], value) : undefined} />
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
          <SourcePlatformRows model={model} variant="compact" />
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
                  className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-none font-medium tabular-nums text-small"
                />
              ) : isSafeHttpUrl(model.sourceUrl) ? (
                <ExternalLink url={model.sourceUrl} label={model.sourceUrl}
                  className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                >
                  {model.sourceUrl}
                </ExternalLink>
              ) : model.sourceUrl ? (
                // Not an http(s) value: plain text, never a clickable link.
                <span className="flex-1 min-w-0 truncate text-right text-[var(--ink-2)]">
                  {model.sourceUrl}
                </span>
              ) : (
                <span className="flex-1 text-right text-[var(--ink-3)]">{t('noValue')}</span>
              )}
              <>{canEdit && <span
                onClick={startEditingSourceUrl}
                className="flex-none w-4 h-4 grid place-items-center rounded-full cursor-pointer text-compact-meta text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
              >
                <Icon name="edit" size={14} />
              </span>}</>
            </span>
          </div>
        </div>

        <div className="px-4 pt-[18px] pb-5">
          <div className="text-compact-meta ui-label text-[var(--ink-3)] pb-2.5">
            {t('hashtagsHeading')}
          </div>
          <div data-tag-list className="flex min-w-0 flex-wrap gap-1.5">
            {model.tags.map((tag) => (
              <span
                key={tag}
                title={`#${tagLabel(tag, language)}`}
                className="inline-flex max-w-full min-w-0 items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-medium tabular-nums text-caption"
              >
                <TagDot hue={tagHues?.[tag]} />
                <span className="whitespace-nowrap">{shortTagLabel(`#${tagLabel(tag, language)}`)}</span>
                {canEdit && <TagRemoveButton
                  tag={tag}
                  autoTag={autoTagBlock.suggested.has(tag)}
                  onRemove={() => onRemoveTag(tag)}
                  onRemoveAndBlock={() => void autoTagBlock.removeAndBlock(tag)}
                  className="flex-none w-4 h-4 grid place-items-center rounded-full cursor-pointer text-compact-meta hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                />}
              </span>
            ))}
            <TagInput
              key={model.id}
              allTags={allTags}
              tags={model.tags}
              onAddTag={onAddTag}
              stripHash
              inputClassName="h-6 w-[118px] px-2.5 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-none font-medium tabular-nums text-caption"
            />
          </div>
          <AutoTagBlockStatus notice={autoTagBlock.notice} error={autoTagBlock.error} onUndo={() => void autoTagBlock.undo()} />
        </div>

        <BuildPlateFit model={model} printers={printers} variant="compact" />
        {import.meta.env.VITE_TARGET !== 'web' && <CompanionSection key={model.id} fileId={model.id} variant="compact" />}
      </div>

      <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        {slicerError && (
          <div className="pb-2 font-medium tabular-nums text-compact-meta text-[var(--accent)] break-words">
            {t('slicerLaunchError')} <ErrorText error={slicerError} />
          </div>
        )}
        {/* Web adds several action buttons that must wrap instead of squeezing each other; the desktop row keeps its fixed layout. */}
        <div className={`flex gap-2${import.meta.env.VITE_TARGET === 'web' ? ' flex-wrap' : ''}`}>
          {confirmDelete ? (
            <>
              <span className="flex-1 flex items-center text-small font-medium text-[var(--ink)]">
                {t('deleteConfirmQuestion')}
              </span>
              <>{!isReadOnly() && <button
                onClick={() => setConfirmDelete(false)}
                className="flex-none h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>}</>
              <>{!isReadOnly() && <button {...lockProps}
                onClick={() => {
                  setConfirmDelete(false);
                  onDelete();
                }}
                className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-none h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer"
              >
                {t('delete')}
              </button>}</>
            </>
          ) : (
            <>
              <RevealFileButton fileId={model.id} compact />
              <WebSlicerMenu key={`slicer-${model.id}`} fileId={model.id} fileName={model.name} fileType={model.fileType} />
              <WebFileActions key={`fileops-${model.id}`} fileIds={[model.id]} name={model.name} />
              <WebRemoveFromCatalogButton key={`remove-${model.id}`} name={model.name} onRemove={onRemoveFromCatalog} />
          {container && <CatalogWebButton key={model.id} fileId={model.id} fileName={model.name} />}
            {!container && (<>{!isReadOnly() && <button
                onClick={() => onOpenInSlicer()}
                className="flex-1 min-w-0 h-8 px-2 truncate rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('openInSlicer')}
              </button>}</>)}
              <>{!isReadOnly() && <button {...lockProps}
                onClick={() => setConfirmDelete(true)}
                aria-label={t('deleteAriaLabel')}
                className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-medium tabular-nums cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                <Icon name="close" size={14} />
              </button>}</>
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
            needsSnapshot={import.meta.env.VITE_TARGET !== 'web' && !(model.hasRenderSnapshot ?? !!model.renderSnapshotImage)}
            onOpenInSlicer={hasSlicer ? onOpenInSlicer : undefined}
            onRemoveFromCatalog={onRemoveFromCatalog}
            onSnapshotCaptured={onSnapshotCaptured}
          />
          <ImageUploadButton onUpload={imageUpload.uploadImage} className="relative m-3 h-9 px-3.5 rounded-lg bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--ink-2)] font-semibold cursor-pointer hover:text-[var(--accent)]" style={{ fontSize: 'var(--font-size-meta)' }} />
        </div>
        <ImageUploadError error={imageUpload.error} onDismiss={imageUpload.dismiss} />

        <div className="px-[18px] pt-4 pb-1 flex items-start justify-between gap-3">
          <div className="font-semibold leading-tight break-words" style={{ fontSize: 'var(--font-size-title)' }}>
            {displayName(model.name)}
          </div>
          <button
            onClick={onToggleFavorite}
            aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
            className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-none text-heading ${model.favorite ? 'text-[var(--accent)]' : 'text-[var(--ink-3)]'}`}
           {...(!canEdit ? { disabled: true, title: t('webReadOnly') } : {})}>
            <Icon name="heart" size={14} fill={model.favorite  ? 'currentColor' : 'none'} />
          </button>
        </div>
        <div className="px-[18px]"><StatusBadges model={model} filament={filament} lastPrinter={lastPrinter} /></div>
        <div data-tag-list className="px-[18px] pb-3.5 flex min-w-0 flex-wrap gap-1.5">
          {model.tags.map((tag) => (
            <span
              key={tag}
              title={`#${tagLabel(tag, language)}`}
              className="inline-flex max-w-full min-w-0 items-center gap-1.5 px-2.5 py-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-meta)' }}
            >
              <TagDot hue={tagHues?.[tag]} />
              <span className="whitespace-nowrap">{shortTagLabel(`#${tagLabel(tag, language)}`)}</span>
              {canEdit && <TagRemoveButton
                tag={tag}
                autoTag={autoTagBlock.suggested.has(tag)}
                onRemove={() => onRemoveTag(tag)}
                onRemoveAndBlock={() => void autoTagBlock.removeAndBlock(tag)}
                className="flex-none w-4 h-4 grid place-items-center rounded-full cursor-pointer hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                style={{ fontSize: 'var(--font-size-label)' }}
              />}
            </span>
          ))}
          <TagInput
            key={model.id}
            allTags={allTags}
            tags={model.tags}
            onAddTag={onAddTag}
            stripHash
            inputClassName="px-2.5 py-1 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-none"
            inputStyle={{ fontSize: 'var(--font-size-meta)' }}
          />
        </div>
        <div className="px-[18px] empty:hidden pb-3.5">
          <AutoTagBlockStatus notice={autoTagBlock.notice} error={autoTagBlock.error} onUndo={() => void autoTagBlock.undo()} />
        </div>

        <div className="px-[18px] pb-4 border-t border-[var(--line)] pt-3.5">
          <div
            className="ui-label text-[var(--ink-3)] mb-2.5"
            style={{ fontSize: 'var(--font-size-label)' }}
          >
            {t('metadataHeading')}
          </div>
          <LicenseEditor key={model.id} value={model.license} onSave={onSetLicense ? value => onSetLicense([model.id], value) : undefined} />
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
          <div style={{ fontSize: 'var(--font-size-body)' }}><SourcePlatformRows model={model} variant="comfort" /></div>
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
                className="flex-1 min-w-0 px-2 py-1 rounded-md border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-none"
              />
            ) : (
              <span className="flex-1 flex items-center justify-end gap-1.5 min-w-0">
                {isSafeHttpUrl(model.sourceUrl) ? (
                  <ExternalLink url={model.sourceUrl} label={model.sourceUrl}
                    className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                  >
                    {model.sourceUrl}
                  </ExternalLink>
                ) : model.sourceUrl ? (
                  // Not an http(s) value: plain text, never a clickable link.
                  <span className="flex-1 min-w-0 truncate text-right text-[var(--ink-2)]">
                    {model.sourceUrl}
                  </span>
                ) : (
                  <span className="flex-1 text-right text-[var(--ink-3)]">{t('noValue')}</span>
                )}
                <>{canEdit && <span
                  onClick={startEditingSourceUrl}
                  className="flex-none w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
                  style={{ fontSize: 'var(--font-size-label)' }}
                >
                  <Icon name="edit" size={14} />
                </span>}</>
              </span>
            )}
          </div>
        </div>

        <BuildPlateFit model={model} printers={printers} variant="comfort" />
        {import.meta.env.VITE_TARGET !== 'web' && <CompanionSection key={model.id} fileId={model.id} variant="comfort" />}
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
            <>{!isReadOnly() && <button
              onClick={() => setConfirmDelete(false)}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-10 px-4 rounded-lg border border-[var(--line-strong)] bg-[var(--panel)] font-semibold cursor-pointer"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {t('cancel')}
            </button>}</>
            <>{!isReadOnly() && <button {...lockProps}
              onClick={() => {
                setConfirmDelete(false);
                onDelete();
              }}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-10 px-4 rounded-lg bg-[var(--accent)] text-[var(--accent-ink)] font-semibold cursor-pointer"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {t('delete')}
            </button>}</>
          </div>
        ) : (
          <>
            <RevealFileButton fileId={model.id} compact />
              <WebSlicerMenu key={`slicer-${model.id}`} fileId={model.id} fileName={model.name} fileType={model.fileType} />
            <WebFileActions key={`fileops-${model.id}`} fileIds={[model.id]} name={model.name} />
              <WebRemoveFromCatalogButton key={`remove-${model.id}`} name={model.name} onRemove={onRemoveFromCatalog} />
          {container && <CatalogWebButton key={model.id} fileId={model.id} fileName={model.name} />}
            {!container && (<>{!isReadOnly() && <button
              onClick={() => onOpenInSlicer()}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-11 px-4 flex items-center gap-2.5 justify-center font-bold cursor-pointer bg-[var(--accent)] text-[var(--accent-ink)] rounded-lg"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              🖨 {t('openInSlicer')}
            </button>}</>)}
            <>{canEdit && <button
              onClick={onToggleQueue}
              className="h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              🎞 {model.queuePosition !== null ? t('removeFromQueue') : t('addToQueue')}
            </button>}</>
            <button
              onClick={onTogglePrintStatus}
              className="h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-body)' }}
             {...(!canEdit ? { disabled: true, title: t('webReadOnly') } : {})}>
              {model.printStatus === 'printed' && <Icon name="check" size={14} />}{t(model.printStatus === 'printed' ? 'markAsNotPrinted' : 'markAsPrinted')}
            </button>
            <>{!isReadOnly() && <button {...lockProps}
              onClick={() => setConfirmDelete(true)}
              aria-label={t('deleteAriaLabel')}
              className="h-9 rounded-lg text-[var(--ink-3)] hover:text-[var(--accent)] cursor-pointer"
              style={{ fontSize: 'var(--font-size-meta)' }}
            >
              {t('deleteAriaLabel')}
            </button>}</>
          </>
        )}
      </div>
    </aside>
  );
}
