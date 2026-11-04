import { useEffect } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { setTrayLabels } from '../lib/api/closeBehavior';

/** Hands the tray-menu texts to the backend at start and after every language change (Rust has no texts of its own). */
export function TrayLabelSync() {
  const { language } = useLanguage();
  const t = useT();
  useEffect(() => {
    setTrayLabels(t('closeActionTrayOpen'), t('closeActionTrayQuit')).catch(() => {});
    // `t` changes with the language only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [language]);
  return null;
}
