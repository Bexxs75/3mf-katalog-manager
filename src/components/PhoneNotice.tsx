import { useState, type ReactNode } from 'react';
import { useT } from '../i18n/LanguageContext';
import { dismissPhoneNotice, isPhoneBrowser, phoneNoticeDismissed } from '../lib/phoneDevice';

// Soft notice, not a block: user-agent detection can be wrong, so the page stays reachable.
export function PhoneNotice({ children }: { children: ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(() => isPhoneBrowser() && !phoneNoticeDismissed());
  if (!open) return <>{children}</>;
  return <main className="min-h-screen grid place-items-center bg-[var(--bg)] text-[var(--ink)]">
    <section className="w-full max-w-sm p-6 border border-[var(--line)] rounded bg-[var(--panel)] space-y-4" role="alert">
      <h1 className="text-xl font-semibold">{t('webPhoneTitle')}</h1>
      <p>{t('webPhoneBody')}</p>
      <button className="p-2 border rounded" onClick={() => { dismissPhoneNotice(); setOpen(false); }}>{t('webPhoneContinue')}</button>
    </section>
  </main>;
}
