import { useEffect, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { Disclosure } from './Disclosure';

/** Matches the server's minimum gap between two exports of one session. */
const COOLDOWN_MS = 30_000;

/**
 * Web catalog backup: a plain download link, so the browser streams the ZIP to disk
 * (the archive can be hundreds of MB, far too much to hold in a fetch response).
 * Server refusals (busy, interval, low disk space) arrive as an error page the browser
 * reports as a failed download; the hint below the button names those causes, and the
 * short local pause keeps a double click from triggering the interval refusal itself. The
 * component cannot tell when the download has finished, so it never reports a finished backup.
 */
export function WebCatalogBackup() {
  const t = useT();
  const [pausedUntil, setPausedUntil] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const paused = pausedUntil > now;
  useEffect(() => {
    if (!paused) return;
    const timer = setTimeout(() => setNow(Date.now()), pausedUntil - Date.now() + 50);
    return () => clearTimeout(timer);
  }, [paused, pausedUntil]);
  const classes = 'block text-center leading-7 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small';
  return (
    <>
      {/* The same element stays mounted while paused, so keyboard focus is not lost. */}
      <a
        href="/api/backup/export"
        download
        aria-disabled={paused ? 'true' : undefined}
        aria-describedby={paused ? 'web-backup-pause' : undefined}
        onClick={(event) => {
          if (paused) { event.preventDefault(); return; }
          const at = Date.now();
          setNow(at);
          setPausedUntil(at + COOLDOWN_MS);
        }}
        className={`${classes} no-underline ${paused ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]'}`}
      >
        {t('exportCatalogWebButton')}
      </a>
      {paused && <p id="web-backup-pause" role="status" className="mt-1.5 text-small text-[var(--ink-2)]">{t('catalogBackupWebPause')}</p>}
      <p className="mt-1.5 text-small text-[var(--ink-2)]">{t('catalogBackupWebHint')}</p>
      <Disclosure summary={t('catalogBackupWebRestoreTitle')} className="mt-1.5">{t('catalogBackupWebRestoreText')}</Disclosure>
      <Disclosure summary={t('catalogBackupWebFailureTitle')} className="mt-1.5">{t('catalogBackupWebFailureHint')}</Disclosure>
    </>
  );
}
