import { useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { usePrintTechnology } from '../hooks/PrintTechnologyContext';

const small = 'px-2.5 py-1 rounded-[4px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[length:var(--font-size-meta)] font-semibold cursor-pointer hover:bg-[var(--panel-2)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/**
 * One-time notice for installations that existed before the question: nothing is
 * hidden until the user chooses. Closing remembers that it was seen, keeping "both".
 */
export function PrintTechnologyNotice({ onChoose }: { onChoose: () => void }) {
  const t = useT();
  const { technology, save } = usePrintTechnology();
  const [failed, setFailed] = useState(false);
  const close = () => {
    // The notice stays when saving fails, so the next start can offer it again.
    save(technology).catch((e) => { console.error('[print-technology] could not remember the notice:', e); setFailed(true); });
    document.querySelector<HTMLElement>('[data-catalog-main]')?.focus();
  };
  return (
    <div role="region" aria-label={t('ptTitle')} className="pointer-events-auto max-w-[min(90vw,480px)] flex items-start gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)]">
      <div className="flex min-w-0 flex-col gap-1.5 [overflow-wrap:anywhere]">
        <span><b>{t('ptTitle')}</b> {t('ptNoticeText')}</span>
        {failed && <span role="alert" className="text-[var(--crit)]">{t('ptSaveFailed')}</span>}
        <div className="flex gap-2">
          <button type="button" className={small} onClick={onChoose}>{t('ptNoticeChoose')}</button>
          <button type="button" className={small} onClick={close}>{t('ptNoticeClose')}</button>
        </div>
      </div>
    </div>
  );
}
