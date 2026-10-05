import { useId, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import type { Translations } from '../i18n/types';
import { isMacPlatform } from '../lib/keyboardGuard';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';

type TextKey = { [K in keyof Translations]: Translations[K] extends string ? K : never }[keyof Translations];

export function KeyboardTipsDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [tab, setTab] = useState(0);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  const id = useId();
  const modifier = isMacPlatform() ? 'Cmd' : t('keyboardCtrl');
  const rows: [string, TextKey][] = [
    ['/', 'keyboardSearch'], [`${modifier}+F`, 'keyboardSearch'], ['← → ↑ ↓', 'keyboardNavigate'],
    ['Enter', 'keyboardEnter'], [t('keyboardHomeEnd'), 'keyboardBoundary'], [t('keyboardSpace'), 'keyboardToggle'],
    [`${modifier}+A`, 'keyboardSelectAll'], [isMacPlatform() ? `${t('keyboardDeleteKey')} / Cmd+⌫` : t('keyboardDeleteKey'), 'keyboardDelete'],
    ['Esc', 'keyboardEscape'], ['Esc', 'keyboardSearchEscape'], ['?', 'keyboardOpenTips'],
  ];
  const tips: [TextKey, TextKey][] = [
    ['tipDragTitle', 'tipDragText'], ['tipBulkTitle', 'tipBulkText'], ['tipContextTitle', 'tipContextText'],
    ['tipSortTitle', 'tipSortText'], ['tipFiltersTitle', 'tipFiltersText'], ['tipSidebarTitle', 'tipSidebarText'], ['tipSinglesTitle', 'tipSinglesText'],
  ];
  return <CatalogActionDialog title={t('keyboardTipsTitle')} onClose={onClose}>
    <div role="tablist" aria-label={t('keyboardTipsTitle')} className="flex gap-1 border-b border-[var(--line)]" onKeyDown={event => {
      if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
      event.preventDefault();
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? 1 : 1 - tab;
      setTab(next); tabs.current[next]?.focus();
    }}>
      {[t('keyboardTab'), t('tipsTab')].map((label, index) => <button key={index} ref={el => { tabs.current[index] = el; }}
        data-initial-focus={index === 0 ? '' : undefined} id={`${id}-tab-${index}`} role="tab" aria-selected={tab === index}
        aria-controls={`${id}-panel-${index}`} tabIndex={tab === index ? 0 : -1} onClick={() => setTab(index)}
        className={`px-3 py-2 border-b-2 cursor-pointer ${tab === index ? 'border-[var(--accent)] text-[var(--ink)]' : 'border-transparent text-[var(--ink-2)]'}`}>{label}</button>)}
    </div>
    <div id={`${id}-panel-0`} role="tabpanel" aria-labelledby={`${id}-tab-0`} hidden={tab !== 0} tabIndex={0}>
      <h3 className="ui-label text-[var(--ink-3)]">{t('keyboardCatalog')}</h3>
      {rows.map(([key, label]) => <div key={label} className="grid grid-cols-1 sm:grid-cols-[150px_1fr] gap-2 py-2 border-b border-[var(--line)]">
        <span><kbd className="font-medium tabular-nums text-caption rounded border border-[var(--line-strong)] bg-[var(--panel-2)] px-1.5 py-0.5">{key}</kbd></span>
        <span className="text-[var(--ink-2)]">{t(label)}</span>
      </div>)}
      <h3 className="mt-3 ui-label text-[var(--ink-3)]">{t('keyboardDetail')}</h3>
      <div className="grid grid-cols-1 sm:grid-cols-[150px_1fr] gap-2 py-2 border-b border-[var(--line)]">
        <span><kbd className="font-medium tabular-nums text-caption rounded border border-[var(--line-strong)] bg-[var(--panel-2)] px-1.5 py-0.5">← →</kbd></span>
        <span className="text-[var(--ink-2)]">{t('keyboardDetailNavigate')}</span>
      </div>
    </div>
    <div id={`${id}-panel-1`} role="tabpanel" aria-labelledby={`${id}-tab-1`} hidden={tab !== 1} tabIndex={0}>
      {tips.map(([title, text]) => <div key={title} className="py-2 border-b border-[var(--line)]"><h3 className="font-semibold">{t(title)}</h3><p className="mt-1 text-[var(--ink-2)]">{t(text)}</p></div>)}
    </div>
    <div className="flex items-center gap-3 border-t border-[var(--line)] pt-3"><p className="flex-1 text-small text-[var(--ink-3)]">{t('keyboardFooter')}</p><button className={catalogActionButton} onClick={onClose}>{t('impClose')}</button></div>
  </CatalogActionDialog>;
}

export function EmptyCatalogTips({ onOpenTips }: { onOpenTips: () => void }) {
  const t = useT();
  return <div className="p-6 text-center space-y-4">
    <h2 className="text-heading font-semibold">{t('emptyCatalogTitle')}</h2><p className="text-[var(--ink-2)]">{t('emptyCatalogText')}</p>
    <button className={catalogActionButton} onClick={onOpenTips}>{t('keyboardTipsTitle')}</button>
    <div className="grid grid-cols-1 md:grid-cols-3 gap-3 max-w-[820px] mx-auto text-left">
      {(['Select', 'Context', 'Search'] as const).map(key => <div key={key} className="border border-[var(--line)] bg-[var(--panel)] rounded-lg p-4">
        <h3 className="font-semibold">{t(`empty${key}Title`)}</h3><p className="mt-1 text-body text-[var(--ink-2)]">{t(`empty${key}Text`).replace('{modifier}', isMacPlatform() ? 'Cmd' : t('keyboardCtrl'))}</p>
      </div>)}
    </div>
  </div>;
}
