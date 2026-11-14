import { useId, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes } from '../i18n/format';
import { displayName } from '../lib/displayName';
import type { OpenRequest } from '../lib/api/openRequests';
import { useImportLock } from '../hooks/ImportLockContext';
import { useModalDialog } from '../hooks/useModalDialog';
import { ModelViewer } from './ModelViewer';
import { Icon } from './Icon';

export type ExternalRequest = Extract<OpenRequest, { kind: 'external' }>;

interface Props {
  /** Everything the system handed over in one go; one external file gets the view, several get the list. */
  requests: OpenRequest[];
  /** Name of a cataloged file for the list (unknown ids fall back to a generic label). */
  nameOfCatalogFile: (fileId: string) => string | undefined;
  /** Starts the normal import for these files; resolves when the start was handed over. */
  onAdd: (files: ExternalRequest[]) => void;
  onClose: () => void;
}

const focusClass = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';
const buttonClass = `h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[length:var(--font-size-control)] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50 disabled:cursor-default ${focusClass}`;
const primaryClass = `h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[length:var(--font-size-control)] font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-default ${focusClass}`;
const pillClass = 'px-2 rounded-full border border-[var(--line-strong)] text-caption text-[var(--ink-2)] whitespace-nowrap';

/**
 * "View only" for files the system opened that are not in the catalog: nothing is moved or
 * stored until the user adds them. One file shows the 3D view, several show a list. The
 * dialog never receives a path, only the source ids and display names.
 */
export function OpenFilesDialog({ requests, nameOfCatalogFile, onAdd, onClose }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const titleId = useId();
  const noteId = useId();
  const { jobActive } = useImportLock();
  const [added, setAdded] = useState(false);
  const dialogRef = useModalDialog({ open: true, onClose, initialFocus: '[data-initial-focus]' });
  const external = requests.filter((r): r is ExternalRequest => r.kind === 'external');
  const single = requests.length === 1 && external.length === 1 ? external[0] : null;
  const known = requests.length - external.length;
  const locked = jobActive || added;
  const add = () => { setAdded(true); onAdd(external); };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}
        className={`${single ? 'w-[560px]' : 'w-[480px]'} max-w-full max-h-full overflow-y-auto bg-[var(--panel)] text-[var(--ink)] border border-[var(--line-strong)] rounded-[10px] shadow-[var(--shadow)] px-4 pt-3.5 pb-4`}>
        <div className="flex items-start gap-2 mb-2">
          <h2 id={titleId} className="m-0 min-w-0 flex-1 text-[length:var(--font-size-title)] font-bold break-words">{single ? t('owViewTitle') : t('owListTitle')}</h2>
          <button type="button" onClick={onClose} aria-label={t('owClose')}
            className={`flex-none w-[30px] h-[30px] grid place-items-center rounded-md text-[var(--ink-2)] hover:bg-[var(--panel-2)] cursor-pointer ${focusClass}`}>
            <Icon name="close" size={16} />
          </button>
        </div>

        {single ? (
          <>
            <div className="mb-2">
              <div className="font-medium break-all">{displayName(single.name)}</div>
              <div className="text-small text-[var(--ink-2)]">{single.extension.toUpperCase()} · {formatBytes(single.sizeBytes, language)}</div>
            </div>
            <div role="group" aria-label={t('owViewerLabel').replace('{name}', displayName(single.name))}
              className="h-[300px] mb-2 rounded-[6px] border border-[var(--line)] overflow-hidden">
              <ModelViewer key={single.sourceId} fileId={single.sourceId} externalSourceId={single.sourceId}
                needsSnapshot={false} onSnapshotCaptured={() => {}} showRotationControls />
            </div>
          </>
        ) : (
          <>
            <p className="m-0 mb-2">{t('owHanded').replace('{total}', String(requests.length)).replace('{known}', String(known))}</p>
            <ul className="m-0 mb-2 p-0 list-none flex flex-col gap-1.5 max-h-[260px] overflow-y-auto">
              {requests.map((request, index) => (
                <li key={request.kind === 'external' ? request.sourceId : `${request.fileId}-${index}`}
                  className="flex items-center gap-2 px-2 py-1.5 rounded-[6px] border border-[var(--line)]">
                  <span className="min-w-0 flex-1 break-all">
                    {displayName(request.kind === 'external' ? request.name : nameOfCatalogFile(request.fileId) ?? t('owCatalogEntry'))}
                  </span>
                  <span className={pillClass}>{request.kind === 'external' ? t('owNew') : t('owInCatalog')}</span>
                </li>
              ))}
            </ul>
            <p className="m-0 mb-2 text-small text-[var(--ink-2)]">{t('owSkipNote')}</p>
          </>
        )}
        <p id={noteId} className="m-0 mb-1 text-small text-[var(--ink-2)]">{t('owNoChange')}</p>
        {jobActive && <p role="status" className="m-0 mb-1 text-small text-[var(--ink-2)]">{t('owLocked')}</p>}
        <div className="flex gap-2 justify-end mt-3">
          <button type="button" onClick={add} disabled={locked || external.length === 0} aria-describedby={noteId} className={primaryClass}>
            {added ? t('owAdding') : t('owAdd')}
          </button>
          <button type="button" data-initial-focus onClick={onClose} className={buttonClass}>{t('owClose')}</button>
        </div>
      </div>
    </div>
  );
}
