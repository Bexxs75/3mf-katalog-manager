import { useEffect, useState } from 'react';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { hasNetworkFilesystemWarning } from '../lib/api/runtime';
import { useT } from '../i18n/LanguageContext';

export function NetworkFilesystemWarning() {
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const [present, setPresent] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  useEffect(() => {
    if (!container) return;
    let active = true;
    void hasNetworkFilesystemWarning().then(value => { if (active) setPresent(value); })
      .catch(error => console.warn('[runtime] could not read filesystem warning:', error));
    return () => { active = false; };
  }, [container]);
  if (!container || !present || dismissed) return null;
  return <div role="alert" className="flex items-center gap-3 px-4 py-3 bg-[var(--warn-soft)] text-[var(--ink)] text-small">
    <p className="flex-1">{t('containerNetworkFilesystemWarning')}</p>
    <button className="shrink-0 underline" onClick={() => setDismissed(true)}>{t('impClose')}</button>
  </div>;
}
