import { useEffect, useRef, useState } from 'react';
import type { ModelFile, SlicerConfig, SyncStatus } from '../types';
import type { Language, Translations } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatDate, formatDimensions, formatRelativeTime, formatVolumeCm3, formatWeightG } from '../i18n/format';
import { ModelViewer } from './ModelViewer';

interface Props {
  model: ModelFile | null;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onTogglePrintStatus: () => void;
  onUploadImage: () => void;
  onSnapshotCaptured: (base64: string) => void;
  onSetSourceUrl: (url: string | null) => void;
  onOpenInSlicer: (slicerId?: string) => void;
  slicers: SlicerConfig[];
  slicerError: string | null;
  onUploadToCloud: () => void;
  cloudUploadAvailable: boolean;
  uploading: boolean;
  cloudUploadError: string | null;
}

type TFunction = <K extends keyof Translations>(key: K) => Translations[K];

const SYNC_KEYS: Record<SyncStatus, 'syncSynced' | 'syncOutdated' | 'syncLocalOnly' | 'syncCloudOnly'> = {
  synced: 'syncSynced',
  outdated: 'syncOutdated',
  'local-only': 'syncLocalOnly',
  'cloud-only': 'syncCloudOnly',
};

function buildMetaRows(model: ModelFile, t: TFunction, language: Language): { label: string; value: string }[] {
  const materialsValue =
    model.materials.length === 0
      ? t('noValue')
      : model.materials.map((m) => m.name).join(', ');
  const objectCountValue = model.objectCount === null ? t('noValue') : String(model.objectCount);
  const weightValue =
    model.estimatedWeightG === null ? t('noValue') : `≈ ${formatWeightG(model.estimatedWeightG, language)}`;

  return [
    { label: t('metaDimensions'), value: formatDimensions(model.dimensionsMm, language) },
    { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    { label: t('metaWeight'), value: weightValue },
    { label: t('metaObjectCount'), value: objectCountValue },
    { label: t('metaMaterial'), value: materialsValue },
    { label: t('metaFileSize'), value: formatBytes(model.fileSizeBytes, language) },
    { label: t('metaImported'), value: formatDate(model.importedAt, language) },
  ];
}

export function DetailPanel({
  model,
  onAddTag,
  onRemoveTag,
  onDelete,
  onTogglePrintStatus,
  onUploadImage,
  onSnapshotCaptured,
  onSetSourceUrl,
  onOpenInSlicer,
  slicers,
  slicerError,
  onUploadToCloud,
  cloudUploadAvailable,
  uploading,
  cloudUploadError,
}: Props) {
  const { language } = useLanguage();
  const t = useT();
  const [draft, setDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [slicerMenuOpen, setSlicerMenuOpen] = useState(false);
  const [editingSourceUrl, setEditingSourceUrl] = useState(false);
  const [sourceUrlDraft, setSourceUrlDraft] = useState('');
  const cancelingSourceUrlRef = useRef(false);

  useEffect(() => {
    setConfirmDelete(false);
    setSlicerMenuOpen(false);
    setEditingSourceUrl(false);
  }, [model?.id]);

  if (!model) {
    return (
      <aside className="flex-none w-[336px] flex items-center justify-center bg-[var(--panel)] border-l border-[var(--line)] text-[var(--ink-3)] text-[13px] px-6 text-center">
        {t('emptyStateText')}
      </aside>
    );
  }

  const submitDraft = () => {
    const value = draft.trim().replace(/^#/, '');
    if (value) onAddTag(value);
    setDraft('');
  };

  const startEditingSourceUrl = () => {
    setSourceUrlDraft(model.sourceUrl ?? '');
    setEditingSourceUrl(true);
  };

  const submitSourceUrl = () => {
    const value = sourceUrlDraft.trim();
    onSetSourceUrl(value || null);
    setEditingSourceUrl(false);
  };

  const hasSlicers = slicers.length > 0;

  const canUpload = model.origin === 'local';
  const uploadDisabled = uploading || !canUpload || !cloudUploadAvailable;
  const uploadAria = uploading
    ? t('uploadingToCloudAria')
    : !canUpload
      ? t('alreadyInCloudAria')
      : !cloudUploadAvailable
        ? t('connectCloudToUploadAria')
        : t('uploadToCloudAria');
  const uploadLabel = uploading
    ? t('uploadButtonLabelInProgress')
    : !canUpload
      ? t('uploadButtonLabelDone')
      : t('uploadButtonLabel');

  return (
    <aside className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
      <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
        <div className="text-[14.5px] font-semibold leading-tight break-words">{model.name}</div>
        <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)] pt-1.5">{model.path}</div>
      </div>

      <div className="flex-1 overflow-y-auto">
        <div className="relative aspect-[4/3] bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 11px)',
            }}
          />
          <ModelViewer
            fileId={model.id}
            needsSnapshot={model.displayImage === null}
            onSnapshotCaptured={onSnapshotCaptured}
          />
          <div className="absolute left-2.5 bottom-2 font-mono-ui text-[9.5px] tracking-[0.08em] uppercase text-[var(--ink-3)] pointer-events-none">
            {t('dragToRotate')}
          </div>
          <button
            onClick={onUploadImage}
            className="absolute right-2.5 top-2.5 h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span
            className={`w-2 h-2 rounded-full ${
              model.sync === 'synced' ? 'bg-[var(--accent)]' : 'bg-[var(--ink-3)]'
            }`}
          />
          <span className="flex-1 text-[12.5px] font-medium">{t(SYNC_KEYS[model.sync])}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
            {formatRelativeTime(model.importedAt, language)}
          </span>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-[12.5px] font-medium">
            {model.printStatus === 'printed' ? t('printedBadge') : t('notPrintedLabel')}
          </span>
          <button
            onClick={onTogglePrintStatus}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.printStatus === 'printed' ? t('markAsNotPrinted') : t('markAsPrinted')}
          </button>
        </div>

        <div className="px-4 pt-3.5 pb-1">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
            {t('metadataHeading')}
          </div>
          {buildMetaRows(model, t, language).map((row) => (
            <div
              key={row.label}
              className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]"
            >
              <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
                {row.label}
              </span>
              <span className="flex-1 font-mono-ui text-xs text-right">{row.value}</span>
            </div>
          ))}
          <div className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]">
            <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
              {t('metaSourceUrl')}
            </span>
            <span className="flex-1 flex items-center justify-end gap-1.5 min-w-0 font-mono-ui text-xs">
              {editingSourceUrl ? (
                <input
                  value={sourceUrlDraft}
                  onChange={(e) => setSourceUrlDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') submitSourceUrl();
                    if (e.key === 'Escape') {
                      cancelingSourceUrlRef.current = true;
                      setEditingSourceUrl(false);
                    }
                  }}
                  onBlur={() => {
                    if (cancelingSourceUrlRef.current) {
                      cancelingSourceUrlRef.current = false;
                      return;
                    }
                    submitSourceUrl();
                  }}
                  autoFocus
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-xs"
                />
              ) : model.sourceUrl ? (
                <a
                  href={model.sourceUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                >
                  {model.sourceUrl}
                </a>
              ) : (
                <span className="flex-1 text-right text-[var(--ink-3)]">{t('noValue')}</span>
              )}
              <span
                onClick={startEditingSourceUrl}
                className="flex-none w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
              >
                ✎
              </span>
            </span>
          </div>
        </div>

        <div className="px-4 pt-[18px] pb-5">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2.5">
            {t('hashtagsHeading')}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {model.tags.map((tag) => (
              <span
                key={tag}
                className="inline-flex items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11.5px]"
              >
                #{tag}
                <span
                  onClick={() => onRemoveTag(tag)}
                  className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                >
                  ✕
                </span>
              </span>
            ))}
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submitDraft()}
              placeholder={t('addTagPlaceholder')}
              className="h-6 w-[118px] px-2.5 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-[11.5px]"
            />
          </div>
        </div>
      </div>

      <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        {slicerError && (
          <div className="pb-2 font-mono-ui text-[10px] text-[var(--accent)] break-words">
            {t('slicerLaunchError')} {slicerError}
          </div>
        )}
        {cloudUploadError && (
          <div className="pb-2 font-mono-ui text-[10px] text-[var(--accent)] break-words">
            {t('cloudUploadError')} {cloudUploadError}
          </div>
        )}
        <div className="flex gap-2">
          {confirmDelete ? (
            <>
              <span className="flex-1 flex items-center text-[12.5px] font-medium text-[var(--ink)]">
                {t('deleteConfirmQuestion')}
              </span>
              <button
                onClick={() => setConfirmDelete(false)}
                className="flex-none h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>
              <button
                onClick={() => {
                  setConfirmDelete(false);
                  onDelete();
                }}
                className="flex-none h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
              >
                {t('delete')}
              </button>
            </>
          ) : (
            <>
              <div className="relative flex flex-1 min-w-0">
                <button
                  onClick={() => onOpenInSlicer()}
                  className={`flex-1 min-w-0 h-8 px-2 truncate border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] ${
                    hasSlicers ? 'rounded-l-[3px] border-r-0' : 'rounded-[3px]'
                  }`}
                >
                  {t('openInSlicer')}
                </button>
                {hasSlicers && (
                  <button
                    onClick={() => setSlicerMenuOpen((o) => !o)}
                    aria-label={t('chooseSlicerAria')}
                    className="flex items-center justify-center w-6 h-8 rounded-r-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                  >
                    <span className="text-[9px] leading-none">▾</span>
                  </button>
                )}
                {slicerMenuOpen && (
                  <div className="absolute bottom-10 left-0 w-[176px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
                    {slicers.map((s) => (
                      <button
                        key={s.id}
                        onClick={() => {
                          setSlicerMenuOpen(false);
                          onOpenInSlicer(s.id);
                        }}
                        className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
                      >
                        {s.name}
                      </button>
                    ))}
                  </div>
                )}
              </div>
              <button
                onClick={() => !uploadDisabled && onUploadToCloud()}
                disabled={uploadDisabled}
                aria-label={uploadAria}
                title={uploadAria}
                className={`flex-none h-8 px-2.5 flex items-center gap-1.5 whitespace-nowrap rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold ${
                  uploadDisabled
                    ? 'opacity-40 cursor-not-allowed'
                    : 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]'
                }`}
              >
                <span className={`font-mono-ui ${uploading ? 'inline-block animate-spin' : 'inline-block'}`}>↑</span>
                {uploadLabel}
              </button>
              <button
                onClick={() => setConfirmDelete(true)}
                aria-label={t('deleteAriaLabel')}
                className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-mono-ui cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                ✕
              </button>
            </>
          )}
        </div>
      </div>
    </aside>
  );
}
