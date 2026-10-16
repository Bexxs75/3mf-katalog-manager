import { useEffect, useState, useSyncExternalStore, type ReactNode, type FormEvent } from 'react';
import * as session from '../lib/webSession';
import { messageOf } from '../lib/errors';
import { useT } from '../i18n/LanguageContext';

export function WebAuthGate({ children }: { children: ReactNode }) {
  const t = useT();
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot);
  const [loading, setLoading] = useState(true);
  const [checked, setChecked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [editing, setEditing] = useState(false);
  const [password, setPassword] = useState('');
  const [next, setNext] = useState('');
  const changing = state.authenticated && (state.mustChangePassword || editing);
  const check = () => { setLoading(true); setChecked(false); setError(''); void session.getSession().then(() => setChecked(true)).catch(e => setError(messageOf(e))).finally(() => setLoading(false)); };
  useEffect(check, []);
  async function submit(event: FormEvent) {
    event.preventDefault(); setBusy(true); setError('');
    try {
      if (changing) { await session.changePassword(password, next); setEditing(false); }
      else await session.login(password);
      setPassword(''); setNext('');
    } catch (e) { setError(messageOf(e)); }
    finally { setBusy(false); }
  }
  if (loading) return <main className="p-6" role="status">{t('webSessionChecking')}</main>;
  if (!checked) return <main className="p-6"><p role="alert">{error}</p><button onClick={check}>{t('webRetry')}</button></main>;
  if (state.authenticated && !changing) return <>
    <div className="fixed bottom-2 right-3 z-50 flex gap-3 rounded bg-[var(--panel)] p-2 text-small">
      <button onClick={() => { setError(''); setEditing(true); }}>{t('webChangePassword')}</button>
      <button disabled={busy} onClick={async () => { setBusy(true); try { await session.logout(); } catch (e) { setError(messageOf(e)); } finally { setBusy(false); } }}>{t('webLogout')}</button>
      {error && <span role="alert">{error}</span>}
    </div>{children}</>;
  return <main className="min-h-screen grid place-items-center bg-[var(--bg)] text-[var(--ink)]">
    <form onSubmit={submit} className="w-full max-w-sm p-6 border border-[var(--line)] rounded bg-[var(--panel)] space-y-4">
      <h1 className="text-xl font-semibold">{t(changing ? 'webChangePassword' : 'webLogin')}</h1>
      {state.mustChangePassword && <p>{t('webPasswordRequired')}</p>}
      <label className="block">{t(changing ? 'webCurrentPassword' : 'webPassword')}
        <input className="block w-full p-2 border rounded bg-[var(--panel-2)]" type="password" autoComplete="current-password" required value={password} onChange={e => setPassword(e.target.value)} />
      </label>
      {changing && <label className="block">{t('webNewPassword')}
        <input className="block w-full p-2 border rounded bg-[var(--panel-2)]" type="password" autoComplete="new-password" required minLength={12} maxLength={128} value={next} onChange={e => setNext(e.target.value)} />
      </label>}
      {error && <p role="alert">{error}</p>}
      <button className="p-2 border rounded" disabled={busy}>{t(changing ? 'webChangePassword' : 'webLogin')}</button>
      {editing && !state.mustChangePassword && <button type="button" onClick={() => { setEditing(false); setPassword(''); setNext(''); }}>{t('webCancel')}</button>}
      {!state.authenticated && error && <button type="button" onClick={check}>{t('webRetry')}</button>}
    </form>
  </main>;
}
