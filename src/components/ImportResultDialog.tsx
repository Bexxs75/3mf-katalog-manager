import { useId, useMemo, useRef, useState, type RefObject } from 'react';
import { useModalDialog } from '../hooks/useModalDialog';
import { useLanguage, useT } from '../i18n/LanguageContext';
import type { ImportJobResult } from '../types';
import type { Translations } from '../i18n/types';
import './import.css';
const reasons: Record<string, keyof Translations> = { empty: 'impEmpty', invalid: 'impInvalid', failed: 'impReadFailed', unsupported: 'impUnsupported', notStarted: 'impNotStarted', cancelled: 'impReasonCancelled', targetMissing: 'impTargetMissing', moveFailed: 'impMoveFailed', protected: 'impProtected', jobFailed: 'impJobFailed', unsafe: 'impUnsafe', blocked: 'impBlocked', existing: 'impExisting', path: 'impPath', hash: 'impHash' };
const tabs = ['impTabSkipped', 'impTabDuplicate', 'impTabNotPlaced', 'impTabArchive'] as const;
interface Row { path: string; reason: string; nested?: boolean; existingFileId?: string | null }
interface Props { result: ImportJobResult; onClose: () => void; returnFocus?: RefObject<HTMLElement | null>; onSelectModel?: (id: string) => void }
export function ImportResultDialog({result, onClose, returnFocus, onSelectModel}: Props) {
  const t = useT(); const {language} = useLanguage(); const id = useId();
  const [tab, setTab] = useState(0); const [copied, setCopied] = useState(false); const [fallback, setFallback] = useState(false);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const dialog = useModalDialog({open: true, onClose, returnFocus, initialFocus: '[data-close]'});
  const rows = useMemo(() => {
    const reason = (key: string) => reasons[key] ? t(reasons[key]) as string : key;
    const duplicate = (r: {path: string; kind: string; existingFileId?: string | null}): Row => ({...r, reason: reason(r.kind) + (r.existingFileId ? ` · ${t('impExistingModel').replace('{id}', r.existingFileId)}` : '')});
    const archive: Row[] = [];
    for (const a of result.groups.archive) {
      archive.push({path: a.path, reason: a.state === 'pending' ? t('impPending') : a.error ? (a.error === 'cancelled' ? t('impArchiveCancelled') : reason(a.error)) : t('impExtracted').replace('{path}', a.extractedTo ?? '')});
      if (a.state !== 'pending') {
        for (const r of a.models.skipped) archive.push({path: r.entryPath, reason: reason(r.reason), nested: true});
        for (const r of a.models.duplicates) archive.push({path: r.entryPath, reason: reason(r.kind), nested: true});
      }
    }
    return [result.groups.skipped.map(r => ({path: r.path, reason: reason(r.reason)})), result.groups.duplicate.map(duplicate), result.groups.importedNotPlaced.map(r => ({path: r.path, reason: reason(r.reason)})), archive] as Row[][];
  }, [result, t]);
  const text = rows.map((group, i) => `${t(tabs[i])}\n${group.map(r => `${r.nested ? '  ' : ''}${r.path}\t${r.reason}`).join('\n')}`).join('\n\n');
  const copy = async () => {
    try { if (!navigator.clipboard?.writeText) throw new Error('clipboard unavailable'); await navigator.clipboard.writeText(text); setCopied(true); setFallback(false); }
    catch { setFallback(true); setCopied(false); }
  };
  const n = (value: number) => new Intl.NumberFormat(language).format(value);
  return <div className="import-backdrop"><div ref={dialog} className="import-result" role="dialog" aria-modal="true" aria-labelledby={`${id}-title`} tabIndex={-1}>
    <h2 id={`${id}-title`}>{t('impResultTitle')}</h2>
    <div className="import-counts">{([[result.counts.imported,'impImported'],[result.counts.duplicate,'impDuplicate'],[result.counts.skipped,'impSkipped'],[result.counts.importedNotPlaced,'impNotPlaced'],[result.counts.archive,'impTabArchive']] as const).map(([value,key]) => <span key={key}><b>{n(value)}</b> {t(key)}</span>)}</div>
    <div role="tablist" aria-label={t('impResultTitle')} className="import-tabs">{tabs.map((key,i) => <button key={key} ref={el => {buttons.current[i] = el;}} id={`${id}-tab-${i}`} role="tab" aria-selected={tab === i} aria-controls={`${id}-panel`} tabIndex={tab === i ? 0 : -1} onClick={() => setTab(i)} onKeyDown={event => {
      const next = event.key === 'ArrowRight' ? (i+1)%4 : event.key === 'ArrowLeft' ? (i+3)%4 : event.key === 'Home' ? 0 : event.key === 'End' ? 3 : null;
      if (next !== null) { event.preventDefault(); setTab(next); buttons.current[next]?.focus(); }
    }}>{t(key)} <span>{n(i === 3 ? result.groups.archive.length : rows[i].length)}</span></button>)}</div>
    <div className="import-table-scroll" role="tabpanel" id={`${id}-panel`} aria-labelledby={`${id}-tab-${tab}`} tabIndex={0}><table><thead><tr><th>{t('impFile')}</th><th>{t('impReason')}</th></tr></thead><tbody>{rows[tab].map((row,i) => <tr key={i} className={row.nested ? 'import-nested' : undefined}><td>{row.nested && '↳ '}{row.path}</td><td>{row.reason}{row.existingFileId && onSelectModel && <button onClick={() => {onClose(); onSelectModel(row.existingFileId!);}}>{t('impExistingModel').replace('{id}', row.existingFileId)}</button>}</td></tr>)}</tbody></table></div>
    {fallback && <label>{t('impCopyFallback')}<textarea readOnly value={text} onFocus={e => e.currentTarget.select()} aria-label={t('impCopyFallback')} /></label>}
    <footer><button onClick={() => void copy()}>{t('impCopy')}</button><span role="status">{copied ? t('impCopied') : ''}</span><button data-close onClick={onClose}>{t('impClose')}</button></footer>
  </div></div>;
}
