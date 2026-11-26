import { Select } from './Select';
import { useEffect, useId, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { displayName } from '../lib/displayName';
import { readPlateOverview } from '../lib/api/files';
import { isReadOnly } from '../lib/readOnly';
import { downloadModelAsStl, exportModelAsStl, showFolderInFileManager, type StlExportResult, type StlFormat, type StlScope } from '../lib/api/stl';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { useModalDialog } from '../hooks/useModalDialog';
import { Icon } from './Icon';

interface Props {
  model: { id: string; name: string; fileType: string };
  /** Opened from the parts list of a STEP file: the parts to save (mesh indices) replace the plate choice. */
  parts?: { indices: number[]; initial: 'parts' | 'eachPart' };
  /** Cancel, Escape or the close button before the save finished. */
  onClose: () => void;
  /** Closing after a finished save; a queue of models continues here, not on cancel. Defaults to `onClose`. */
  onFinished?: () => void;
}

const focusClass = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';
const buttonClass = `h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[length:var(--font-size-control)] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50 ${focusClass}`;
const primaryClass = `h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[length:var(--font-size-control)] font-semibold cursor-pointer disabled:opacity-50 ${focusClass}`;

/**
 * "Save as STL": range (only where there is a choice), hints, then the system save dialog opens in the
 * backend. Nothing is added to the catalog; an existing file is never overwritten.
 */
export function StlExportDialog({ model, parts, onClose, onFinished }: Props) {
  const t = useT();
  const titleId = useId();
  const scopeName = useId();
  const formatName = useId();
  const [phase, setPhase] = useState<'set' | 'saving' | 'done'>('set');
  const [plates, setPlates] = useState<number[]>([]);
  const [wholeModel, setWholeModel] = useState(true);
  const [plate, setPlate] = useState<number | null>(null);
  const [format, setFormat] = useState<StlFormat>('binary');
  const [eachPart, setEachPart] = useState(parts?.initial === 'eachPart');
  const [error, setError] = useState<AppError | null>(null);
  const [result, setResult] = useState<StlExportResult | null>(null);
  const [folderError, setFolderError] = useState<AppError | null>(null);
  const closeDone = onFinished ?? onClose;
  const dialogRef = useModalDialog({ open: true, onClose: () => (phase === 'done' ? closeDone() : onClose()), busy: phase === 'saving', initialFocus: '[data-initial-focus]' });
  const type = model.fileType.toLowerCase();
  const web = isReadOnly();

  // Only a 3MF has plates; the list comes from the file when the dialog opens. A failed read just
  // leaves out the choice, the whole model can still be saved.
  useEffect(() => {
    if (type !== '3mf') return;
    let current = true;
    readPlateOverview(model.id, false).then(
      answer => { if (current) setPlates(answer.plates.map(row => row.plateIndex).sort((a, b) => a - b)); },
      () => {},
    );
    return () => { current = false; };
  }, [model.id, type]);

  const chosenPlate = plate ?? plates[0] ?? null;
  const manyPlates = plates.length > 1;
  const scope: StlScope = parts ? (eachPart ? { kind: 'eachPart' } : { kind: 'parts', indices: parts.indices })
    : manyPlates && !wholeModel && chosenPlate !== null ? { kind: 'plate', number: chosenPlate } : { kind: 'all' };

  const save = async () => {
    setError(null);
    setPhase('saving');
    try {
      const saved = web ? await downloadModelAsStl(model.id, model.name, scope, format) : await exportModelAsStl(model.id, scope, format);
      if (saved === null) { setPhase('set'); return; }
      setResult(saved);
      setPhase('done');
    } catch (e) {
      setError(toAppError(e));
      setPhase('set');
    }
  };

  const noteColors = type === '3mf' || type === 'stp' || type === 'step';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}
        className="w-[440px] max-w-full max-h-full overflow-y-auto bg-[var(--panel)] text-[var(--ink)] border border-[var(--line-strong)] rounded-[10px] shadow-[var(--shadow)] px-4 pt-3.5 pb-4">
        <div className="flex items-start gap-2 mb-2">
          <h2 id={titleId} className="m-0 min-w-0 flex-1 text-[length:var(--font-size-title)] font-bold break-words">{t('stDialogTitle')}</h2>
          <button type="button" onClick={() => (phase === 'done' ? closeDone() : onClose())} disabled={phase === 'saving'} aria-label={t('stClose')}
            className={`flex-none w-[30px] h-[30px] grid place-items-center rounded-md text-[var(--ink-2)] hover:bg-[var(--panel-2)] cursor-pointer disabled:opacity-50 ${focusClass}`}>
            <Icon name="close" size={16} />
          </button>
        </div>
        <div className="font-medium break-all mb-3">{displayName(model.name)}</div>

        {phase === 'done' && result ? (
          <>
            <p role="status" className="m-0 mb-1 font-semibold">{t(web ? 'stDownloaded' : 'stDone')}</p>
            <p className="m-0 mb-2 font-code text-small break-all text-[var(--ink-2)]">{result.fileCount > 1 ? t('stDoneMany').replace('{n}', String(result.fileCount)) : displayName(result.name)}</p>
            {result.inCatalogFolder && <p className="m-0 mb-2 text-small text-[var(--ink-2)]">{t('stDoneInCatalog')}</p>}
            {folderError && <p role="alert" className="m-0 mb-2 text-small text-[var(--accent)]"><ErrorText error={folderError} /></p>}
            <div className="flex gap-2 justify-end mt-3">
              {!web && <button type="button" className={buttonClass}
                onClick={() => { setFolderError(null); showFolderInFileManager(result.folder).catch(e => setFolderError(toAppError(e))); }}>
                {t('showInFileManager')}
              </button>}
              <button type="button" data-initial-focus onClick={closeDone} className={primaryClass}>{t('stClose')}</button>
            </div>
          </>
        ) : (
          <>
            {parts && (
              <fieldset className="m-0 mb-3 p-0 border-0">
                <legend className="ui-label text-[var(--ink-3)] mb-1">{t('stScope')}</legend>
                <label className="flex items-center gap-2 py-1 cursor-pointer">
                  <input type="radio" name={scopeName} checked={!eachPart} disabled={phase === 'saving' || parts.indices.length === 0} onChange={() => setEachPart(false)} />
                  {t('stScopeParts').replace('{n}', String(parts.indices.length))}
                </label>
                <label className="flex items-center gap-2 py-1 cursor-pointer">
                  <input type="radio" name={scopeName} checked={eachPart} disabled={phase === 'saving'} onChange={() => setEachPart(true)} />
                  {t('stScopeEach')}
                </label>
              </fieldset>
            )}
            {!parts && manyPlates && (
              <fieldset className="m-0 mb-3 p-0 border-0">
                <legend className="ui-label text-[var(--ink-3)] mb-1">{t('stScope')}</legend>
                <label className="flex items-center gap-2 py-1 cursor-pointer">
                  <input type="radio" name={scopeName} checked={wholeModel} disabled={phase === 'saving'} onChange={() => setWholeModel(true)} />
                  {t('stScopeAll')}
                </label>
                <label className="flex items-center gap-2 py-1 cursor-pointer">
                  <input type="radio" name={scopeName} checked={!wholeModel} disabled={phase === 'saving'} onChange={() => setWholeModel(false)} />
                  {t('stScopePlate')}
                  <Select value={chosenPlate ?? ''} disabled={wholeModel || phase === 'saving'} aria-label={t('stPlate')}
                    onValueChange={value => setPlate(Number(value))}
                    className="h-7 px-1 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)]">
                    {plates.map(number => <option key={number} value={number}>{t('stPlate')} {number}</option>)}
                  </Select>
                </label>
              </fieldset>
            )}
            <fieldset className="m-0 mb-3 p-0 border-0">
              <legend className="ui-label text-[var(--ink-3)] mb-1">{t('stFormat')}</legend>
              <label className="flex items-center gap-2 py-1 cursor-pointer">
                <input type="radio" name={formatName} checked={format === 'binary'} disabled={phase === 'saving'} onChange={() => setFormat('binary')} />
                {t('stFormatBinary')}
              </label>
              <label className="flex items-center gap-2 py-1 cursor-pointer">
                <input type="radio" name={formatName} checked={format === 'ascii'} disabled={phase === 'saving'} onChange={() => setFormat('ascii')} />
                {t('stFormatAscii')}
              </label>
              {format === 'ascii' && <p className="m-0 mt-1 text-small text-[var(--ink-2)]">{t('stFormatAsciiNote')}</p>}
            </fieldset>
            {noteColors && <p className="m-0 mb-1 text-small text-[var(--ink-2)]">{t('stNoteColors')}</p>}
            {!parts && manyPlates && wholeModel && <p className="m-0 mb-1 text-small text-[var(--ink-2)]">{t('stNotePlates')}</p>}
            {phase === 'saving' && <p role="status" className="m-0 mt-2 text-small">{t('stSaving')}</p>}
            {error && <p role="alert" className="m-0 mt-2 text-small text-[var(--accent)]"><ErrorText error={error} /></p>}
            <div className="flex gap-2 justify-end mt-3">
              <button type="button" onClick={onClose} disabled={phase === 'saving'} className={buttonClass}>{t('cancel')}</button>
              <button type="button" data-initial-focus onClick={() => void save()} disabled={phase === 'saving'} className={primaryClass}>{t(web ? 'stDownload' : 'stNext')}</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
