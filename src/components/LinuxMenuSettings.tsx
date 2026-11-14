import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import * as api from '../lib/api/linuxMenu';
import { toAppError, type AppError } from '../lib/errors';
import { SettingLoadStatus, SettingSaveStatus, useLoadedSetting } from './SettingState';
import { ToggleSwitch } from './ToggleSwitch';
import { useSectionContent } from './SettingsSection';

/** Settings switch for the start-menu entry; renders nothing unless this is a Linux AppImage. */
export function LinuxMenuSettings() {
  const t = useT();
  const { value: status, confirm, state: loadState, error: loadError, reload } = useLoadedSetting(api.getLinuxMenuStatus);
  const [busy, setBusy] = useState(false);
  const [justAdded, setJustAdded] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mountedRef = useRef(true);
  // Set to true again on setup: StrictMode runs setup, cleanup, setup, and a
  // cleanup-only reset would leave the ref false for the real mount.
  useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false; }; }, []);
  // Tells a section around this block whether there is anything to show: a read in progress or failed is, a loaded "no" is not.
  useSectionContent(loadState !== 'ready' || Boolean(status));
  // `null` is the loaded answer "not a Linux AppImage": nothing is shown then, unlike a failed read.
  if (!status && loadState !== 'ready') return <SettingLoadStatus state={loadState} label={t('linuxMenuTitle')} error={loadError} onReload={() => void reload()} className="mt-4 mb-2" />;
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
      if (next) confirm(next);
      setJustAdded(enabled && next?.installed === true);
    } catch (e) {
      if (mountedRef.current) setError(toAppError(e));
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }
  return <>
    <SettingLoadStatus keptValue state={loadState} error={loadError} onReload={() => void reload()} />
    <div className="flex items-center justify-between gap-2 mt-4 mb-2">
      <span className="text-[length:var(--font-size-body)] font-semibold">{t('linuxMenuTitle')}</span>
      <ToggleSwitch checked={status.installed} label={t('linuxMenuTitle')} disabled={blocked || loadState !== 'ready'} busy={busy} onChange={on => void change(on)} />
    </div>
    <p className="mt-1 text-caption leading-[1.4] text-[var(--ink-3)]">
      {status.thirdParty && !status.installed ? t('linuxMenuThirdParty')
        : status.conflict && !status.installed
          ? (status.conflictPath ? t('linuxMenuForeignFilePath').replace('{path}', status.conflictPath) : t('linuxMenuForeignFile'))
        : t('linuxMenuHint')}
    </p>
    {justAdded && <p role="status" className="mt-1 text-caption leading-[1.4] text-[var(--ink-2)]">{t('linuxMenuAdded')}</p>}
    {/* Files are written here, so after a failure the real state is read again rather than promised. */}
    <SettingSaveStatus saving={busy} error={error} keptValue={false} onReload={() => { setError(null); void reload(); }} className="mt-1" />
  </>;
}
