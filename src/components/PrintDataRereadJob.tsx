import { useEffect, useRef, type RefObject } from 'react';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { usePrintDataReread, type RereadOrigin } from '../hooks/usePrintDataReread';

/** Files between two announcements of the progress bar to a screen reader. */
const ANNOUNCE_EVERY = 10;

interface Props {
  origin: RereadOrigin;
  /** Takes the focus when the run ends: the control that started it (it is active again by then). */
  returnFocus: RefObject<HTMLElement | null>;
}

/**
 * Progress, cancel button and result of the bulk read. Shared by the settings block and the
 * selection bar; only the surface that started the run shows it.
 */
export function PrintDataRereadJob({ origin, returnFocus }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { language } = useLanguage();
  const { state, cancel } = usePrintDataReread();
  const root = useRef<HTMLDivElement>(null);
  const cancelButton = useRef<HTMLButtonElement>(null);
  const mine = state.origin === origin;
  const active = mine && (state.status === 'running' || state.status === 'cancelling');
  const ended = mine && (state.status === 'done' || state.status === 'cancelled');

  // Starting hides the trigger's action, so the cancel button takes over the focus; at the end it goes back.
  const wasActive = useRef(false);
  useEffect(() => {
    if (active && !wasActive.current) cancelButton.current?.focus();
    if (!active && wasActive.current) {
      const focused = document.activeElement;
      if (!focused || focused === document.body || root.current?.contains(focused)) returnFocus.current?.focus();
    }
    wasActive.current = active;
  }, [active, returnFocus]);

  const n = (value: number) => new Intl.NumberFormat(language).format(value);
  const fill = (text: string, values: Record<string, number>) => Object.entries(values).reduce((out, [key, value]) => out.replace(`{${key}}`, n(value)), text);
  const busyNotice = state.busyNotice === origin;

  let progressText = '';
  let announceText = '';
  if (active) {
    const current = Math.min(state.done + 1, state.total);
    progressText = state.status === 'cancelling' ? t('printDataRereadCancelling')
      : state.paused ? t('printDataRereadStoppedImport')
      : fill(t('printDataRereadProgress'), { n: current, total: state.total });
    // The bar itself is not live: only every tenth file, a pause or a cancel is announced.
    const announcedFiles = Math.floor(state.done / ANNOUNCE_EVERY) * ANNOUNCE_EVERY;
    announceText = state.status === 'cancelling' || state.paused ? progressText
      : announcedFiles > 0 ? fill(t('printDataRereadProgress'), { n: announcedFiles, total: state.total }) : '';
  }

  const succeeded = state.done - state.failed;
  let resultText = '';
  if (ended) {
    if (state.status === 'cancelled') resultText = fill(t('printDataRereadCancelled'), { n: state.done, total: state.total });
    else if (state.total === 0) resultText = t('printDataRereadNothing');
    else if (succeeded > 0) {
      resultText = state.partial > 0
        ? formatCount(t('printDataRereadDone'), succeeded).replace('{p}', n(state.partial))
        : formatCount(t('printDataRereadDoneClean'), succeeded);
    }
  }
  const resultHasText = ended && (resultText !== '' || state.failed > 0);

  return (
    <div ref={root} className={active || resultHasText || busyNotice ? 'mt-2.5 grid gap-1.5' : undefined}>
      {active && <span role="status" aria-live="polite" className="sr-only">{announceText}</span>}
      {active && (
        <>
          <div
            role="progressbar" aria-label={t('printDataRereadTitle')}
            aria-valuemin={0} aria-valuemax={state.total} aria-valuenow={Math.min(state.done, state.total)}
            className="h-2 rounded-full bg-[var(--panel-2)] border border-[var(--line)] overflow-hidden"
          >
            <div className="h-full bg-[var(--accent)]" style={{ width: `${state.total ? Math.round(state.done / state.total * 100) : 0}%` }} />
          </div>
          <div className="flex flex-wrap items-center justify-between gap-2 text-small text-[var(--ink-2)]">
            <span className="tabular-nums">{progressText}</span>
            <button
              ref={cancelButton} type="button" onClick={cancel} disabled={state.status === 'cancelling'}
              className="h-7 px-3 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50 disabled:cursor-default focus-visible:outline focus-visible:outline-[3px] focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]"
            >
              {t('cancel')}
            </button>
          </div>
        </>
      )}
      {resultHasText && (
        <div role="status" className="grid gap-1 text-small text-[var(--ink-2)]">
          {resultText && <span>{resultText}</span>}
          {state.failed > 0 && (
            <span className="text-[var(--accent)]">
              {formatCount(t('printDataRereadFailed'), state.failed)} <ErrorText error={state.firstError} />
            </span>
          )}
        </div>
      )}
      {busyNotice && <span role="status" className="text-small text-[var(--ink-2)]">{t('printDataRereadBusy')}</span>}
    </div>
  );
}
