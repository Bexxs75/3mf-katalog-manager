import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';

export function MigrationBackupWarning() {
  const t = useT();
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    let cancelled = false;
    void invoke<boolean>('get_migration_backup_warning').then(failed => {
      if (!cancelled) setVisible(failed);
    }).catch(error => console.warn('[startup] reading migration backup warning failed:', error));
    return () => { cancelled = true; };
  }, []);
  if (!visible) return null;
  return <div role="alert" className="fixed top-4 left-1/2 -translate-x-1/2 z-[100] max-w-lg rounded border border-[var(--warn)] bg-[var(--panel)] p-4 text-small text-[var(--ink)] shadow-lg">
    <p>{t('migrationBackupWarning')}</p>
    <button className="mt-2 rounded border border-[var(--line)] px-3 py-1" onClick={() => setVisible(false)}>{t('impClose')}</button>
  </div>;
}
