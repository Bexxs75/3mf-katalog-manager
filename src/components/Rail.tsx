import { useEffect, useRef } from 'react';
import type { MainView } from '../types';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useT } from '../i18n/LanguageContext';
import { Icon } from './Icon';

interface Props {
  mainView: MainView;
  onMainViewChange: (v: MainView) => void;
  trashCount: number;
  settingsOpen: boolean;
  onSettingsOpenChange: (open: boolean) => void;
}

const railBtnBase =
  'relative w-[42px] h-[42px] rounded-[10px] border border-transparent grid place-items-center cursor-pointer text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]';
const railBtnActive =
  'text-[var(--accent)] bg-[var(--accent-soft)] border-[color-mix(in_oklch,var(--accent)_30%,transparent)]';

export function Rail({ mainView, onMainViewChange, trashCount, settingsOpen, onSettingsOpenChange }: Props) {
  const t = useT();
  const { container } = useRuntimeEnvironment();
  const settingsButtonRef = useRef<HTMLButtonElement>(null);
  const wasOpen = useRef(settingsOpen);
  useEffect(() => {
    // Closing removes the panel together with the focused element; hand focus back to the
    // gear unless the user already moved it somewhere real.
    const active = document.activeElement;
    if (wasOpen.current && !settingsOpen && (!active || active === document.body)) settingsButtonRef.current?.focus();
    wasOpen.current = settingsOpen;
  }, [settingsOpen]);

  return (
    <nav className="flex-none w-[60px] flex flex-col items-center pt-3.5 pb-2.5 bg-[var(--panel-2)] border-r border-[var(--line)]">
      <div className="flex flex-col gap-1.5">
        <button
          onClick={() => onMainViewChange('catalog')}
          title={t('railCatalog')}
          aria-label={t('railCatalog')}
          className={`${railBtnBase} ${mainView === 'catalog' ? railBtnActive : ''}`}
        >
          <Icon name="catalog" />
        </button>
        <button
          onClick={() => onMainViewChange('filament')}
          title={t('railFilament')}
          aria-label={t('railFilament')}
          className={`${railBtnBase} ${mainView === 'filament' ? railBtnActive : ''}`}
        >
          <Icon name="spool" />
        </button>
        <button onClick={() => onMainViewChange('printers')} title={t('railPrinters')} aria-label={t('railPrinters')}
          className={`${railBtnBase} ${mainView === 'printers' ? railBtnActive : ''}`}>
          <Icon name="printer" />
        </button>
        {container && <button type="button" onClick={() => onMainViewChange('downloads')}
          title={t('railDownloads')} aria-label={t('railDownloads')}
          className={`${railBtnBase} ${mainView === 'downloads' ? railBtnActive : ''}`}>
          <Icon name="import" />
        </button>}
        <button
          onClick={() => onMainViewChange('trash')}
          title={t('trashHeading')}
          aria-label={t('trashHeading')}
          className={`${railBtnBase} ${mainView === 'trash' ? railBtnActive : ''}`}
        >
          <Icon name="trash" />
          {trashCount > 0 && (
            <span className="absolute -top-1 -right-1 min-w-[15px] h-[15px] px-[3px] rounded-full bg-[var(--accent)] text-[var(--accent-ink)] text-compact-label font-semibold tabular-nums grid place-items-center">
              {trashCount}
            </span>
          )}
        </button>
      </div>

      <div className="flex-1" />

      <button
        ref={settingsButtonRef}
        onClick={() => onSettingsOpenChange(!settingsOpen)}
        aria-expanded={settingsOpen}
        title={t('settingsTitle')}
        aria-label={t('settingsTitle')}
        className={`${railBtnBase} ${railBtnActive}`}
      >
        <Icon name="settings" />
      </button>
    </nav>
  );
}
