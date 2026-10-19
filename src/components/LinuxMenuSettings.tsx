import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import * as api from '../lib/api/linuxMenu';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { ToggleSwitch } from './ToggleSwitch';

/** Settings switch for the start-menu entry; renders nothing unless this is a Linux AppImage. */
export function LinuxMenuSettings() {
  const t = useT();
  const [status, setStatus] = useState<api.LinuxMenuStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [justAdded, setJustAdded] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mountedRef = useRef(true);
  useEffect(() => () => { mountedRef.current = false; }, []);
  useEffect(() => {
    api.getLinuxMenuStatus().then(s => mountedRef.current && setStatus(s)).catch(() => {});
  }, []);
  if (!status) return null;
  // Without our own entry, a foreign file or another tool's integration blocks the switch.
  const blocked = !status.installed && (status.thirdParty || status.conflict);
  async function change(enabled: boolean) {
    setBusy(true);
    setError(null);
    setJustAdded(false);
    try {
      const next = await api.setLinuxMenuEnabled(enabled);
      if (!mountedRef.current) return;
      if (next) setStatus(next);
      setJustAdded(enabled && next?.installed === true);
    } catch (e) {
      if (mountedRef.current) setError(toAppError(e));
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }
  return <>
    <div className="flex items-center justify-between gap-2 mt-4 mb-2">
      <span className="text-[length:var(--font-size-body)] font-semibold">{t('linuxMenuTitle')}</span>
      <ToggleSwitch checked={status.installed} label={t('linuxMenuTitle')} disabled={busy || blocked} onChange={on => void change(on)} />
    </div>
    <p className="mt-1 text-caption leading-[1.4] text-[var(--ink-3)]">
      {status.thirdParty && !status.installed ? t('linuxMenuThirdParty')
        : status.conflict && !status.installed
          ? (status.conflictPath ? t('linuxMenuForeignFilePath').replace('{path}', status.conflictPath) : t('linuxMenuForeignFile'))
        : t('linuxMenuHint')}
    </p>
    {justAdded && <p role="status" className="mt-1 text-caption leading-[1.4] text-[var(--ink-2)]">{t('linuxMenuAdded')}</p>}
    {error && <p role="alert" className="mt-1 text-caption text-[var(--crit)]">{t('linuxMenuFailed')} <ErrorText error={error} /></p>}
  </>;
}
