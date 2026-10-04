import { createContext, useContext } from 'react';
import { useT } from '../i18n/LanguageContext';

export const ImportLockContext = createContext(false);
export function useImportLock() {
  const jobActive = useContext(ImportLockContext);
  const t = useT();
  return { jobActive, lockProps: jobActive ? { disabled: true, title: t('importLockedHint') } : {} };
}
