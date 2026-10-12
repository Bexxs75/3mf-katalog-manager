import { displayName } from '../lib/displayName';
import { useEffect, useRef, useState } from 'react';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useT } from '../i18n/LanguageContext';
import { getCatalogWebLink, type CatalogWebLink } from '../lib/api/runtime';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { ExternalLink } from './ExternalLink';

function CatalogWebDialog({ fileId, fileName, onClose, returnFocus }: {
  fileId: string; fileName?: string; onClose: () => void; returnFocus: React.RefObject<HTMLButtonElement | null>;
}) {
  const t = useT();
  const [link, setLink] = useState<CatalogWebLink | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true;
    void getCatalogWebLink(fileId).then(result => {
      if (active) setLink(result);
    }).catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [fileId]);
  return <CatalogActionDialog title={t('catalogWebTitle')} onClose={onClose} returnFocus={returnFocus}>
    <p>{t('catalogWebExplanation')}</p>
    {failed ? <p role="alert">{t('catalogWebFailed')}</p> : link ? <>
      {fileName && <p>{t('catalogWebSearchHint')
        .replace('{address}', () => displayName((link.url ?? link.path).split('?')[0]))
        .replace('{fileName}', () => displayName(fileName))}</p>}
      {link.url ? <><div><ExternalLink url={link.url} label={t('catalogWebTitle')} /></div><p>{t('catalogWebPasteHint')}</p></> : <>
        <p className="break-all" style={{ userSelect: 'text' }}>{displayName(link.path)}</p>
        <p>{t('catalogWebUnknown')}</p>
        <p>{t('catalogWebConfigure')}</p>
      </>}
    </> : <p role="status">{t('catalogWebLoading')}</p>}
    <div className="flex justify-end">
      <button type="button" data-initial-focus className={catalogActionButton} onClick={onClose}>{t('impClose')}</button>
    </div>
  </CatalogActionDialog>;
}

export function CatalogWebButton({ fileId, fileName }: { fileId: string; fileName?: string }) {
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const [openedFile, setOpenedFile] = useState<string | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  if (!container) return null;
  return <>
    <button type="button" ref={button} className={catalogActionButton} title={t('catalogWebTitle')}
      aria-haspopup="dialog" onClick={() => setOpenedFile(fileId)}>{t('catalogWebOpen')}</button>
    {openedFile === fileId && <CatalogWebDialog key={fileId} fileId={fileId} fileName={fileName} onClose={() => setOpenedFile(null)} returnFocus={button} />}
  </>;
}
