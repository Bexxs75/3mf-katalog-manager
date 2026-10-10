import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { Icon } from './Icon';
import { useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ReportProblemLink } from '../diagnostics/ReportProblemLink';
import { ErrorText } from '../diagnostics/ErrorText';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { useImportLock } from '../hooks/ImportLockContext';
import { useModelImages } from '../hooks/useModelImages';
import type { ModelFile } from '../types';

export interface ViewerActions {
  model?: ModelFile;
  onOpenInSlicer?: () => void;
  onRemoveFromCatalog?: () => Promise<void>;
}

export function ViewerErrorCard({ error, noWebGL, compact, model, onOpenInSlicer, onRemoveFromCatalog }: ViewerActions & {
  error: AppError | null; noWebGL: boolean; compact: boolean;
}) {
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const { lockProps } = useImportLock();
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<AppError | null>(null);
  const removeButton = useRef<HTMLButtonElement>(null);
  const images = useModelImages(model ? [model.id] : []);
  const sources = model ? images.get(model.id) ?? model : null;
  const image = sources?.customImage ?? sources?.thumbnailImage ?? sources?.renderSnapshotImage;
  const code = error?.code;
  const titles = { notFound: 'viewerNotFoundTitle', unreadable: 'viewerUnreadableTitle', tooLarge: 'viewerTooLargeTitle', unsupported: 'viewerUnsupportedTitle' } as const;
  const texts = { notFound: 'viewerNotFoundText', unreadable: 'viewerUnreadableText', tooLarge: 'viewerTooLargeText', unsupported: 'viewerUnsupportedText' } as const;
  const tone = code === 'notFound' ? 'warn' : code === 'unreadable' ? 'crit' : 'unk';
  const runAction = async (action: () => void | Promise<unknown>) => {
    setActionError(null);
    try { await action(); } catch (e) { setActionError(toAppError(e)); }
  };
  const openFolder = () => {
    if (!model) return;
    // The existing command accepts directories only, including Windows paths.
    let path = model.path.replace(/[\\/][^\\/]*$/, '') || '/';
    if (/^[A-Za-z]:$/.test(path)) path += '\\';
    return invoke('open_in_file_manager', { path });
  };
  return <>
    {image && <img src={image} alt={model?.name ?? ''} className="absolute inset-0 w-full h-full object-contain" />}
    <div className="absolute inset-0 flex flex-col justify-start items-center p-3 pt-12 overflow-auto">
      <div role="alert" className={`viewer-error-card shrink-0 my-auto ${compact ? 'viewer-error-compact' : ''}`}>
        <div className={compact ? 'flex items-center gap-2' : 'contents'}>
          <span aria-hidden="true" className="viewer-error-icon" style={{ color: `var(--${tone})`, background: `var(--${tone}-soft)` }}><Icon name={tone === 'crit' ? 'close' : tone === 'warn' ? 'warning' : 'info'} size={compact ? 24 : 18} /></span>
          <h3>{noWebGL ? t('viewerNoWebGLTitle') : code ? t(titles[code]) : t('previewUnavailable')}</h3>
        </div>
        <p>{noWebGL ? t(compact ? 'viewerNoWebGLCompact' : 'viewerNoWebGLText')
          : compact && code === 'unreadable' ? t('viewerUnreadableCompact')
          : compact && code === 'tooLarge' ? t('viewerTooLargeCompact')
          : code ? t(texts[code]) : t('viewerUnknownText')}</p>
        {code === 'notFound' && model && <code title={model.path} className={`${compact ? 'truncate' : 'break-all'} font-code text-[var(--ink-2)]`}>{model.path}</code>}
        <div className="flex flex-wrap gap-2">
          {!container && code === 'notFound' && model && <button className={catalogActionButton} onClick={() => void runAction(openFolder)}>{t('viewerOpenFolder')}</button>}
          {code === 'notFound' && onRemoveFromCatalog && <button ref={removeButton} {...lockProps} className={catalogActionButton} onClick={() => setConfirm(true)}>{t('removeCatalog')}</button>}
          {!container && code !== 'notFound' && onOpenInSlicer && <button className={catalogActionButton} onClick={() => void runAction(onOpenInSlicer)}>{t('viewerOpenSlicer')}</button>}
          {error?.unexpected && !noWebGL && <ReportProblemLink />}
        </div>
        {actionError && <ErrorText error={actionError} />}
      </div>
    </div>
    {confirm && <CatalogActionDialog title={t('removeModelQuestion').replace('{name}', model?.name ?? '')} onClose={() => { if (!busy) setConfirm(false); }} returnFocus={removeButton}>
      <p>{t('removeModelHint')}</p>
      {actionError && <ErrorText error={actionError} />}
      <div className="flex justify-end gap-2">
        <button data-initial-focus disabled={busy} className={catalogActionButton} onClick={() => setConfirm(false)}>{t('cancel')}</button>
        <button disabled={busy} {...lockProps} className={catalogActionButton} onClick={async () => {
          if (!onRemoveFromCatalog) return;
          setBusy(true); setActionError(null);
          try { await onRemoveFromCatalog(); setConfirm(false); }
          catch (e) { setActionError(toAppError(e)); }
          finally { setBusy(false); }
        }}>{t('removeEntry')}</button>
      </div>
    </CatalogActionDialog>}
  </>;
}
