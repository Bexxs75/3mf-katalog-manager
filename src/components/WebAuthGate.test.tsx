import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { LanguageProvider, useLanguage } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { fillParams } from '../lib/errorMessage';
import { WebAuthGate } from './WebAuthGate';
import { authenticationFailed, getSession } from '../lib/webSession';
afterEach(() => { vi.unstubAllGlobals(); localStorage.clear(); });
it('checks session, requires login and password change before mounting the app', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(Response.json({ authenticated: false, mustChangePassword: false, csrfToken: null }))
    .mockResolvedValueOnce(Response.json({ mustChangePassword: true, csrfToken: 'a' }))
    .mockResolvedValueOnce(Response.json({ csrfToken: 'b' })));
  await act(async () => { render(<LanguageProvider><WebAuthGate><div>APP</div></WebAuthGate></LanguageProvider>); });
  expect(screen.queryByText('APP')).toBeNull();
  fireEvent.change(screen.getByLabelText('Passwort'), { target: { value: 'old password' } });
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Anmelden' })));
  expect(screen.queryByText('APP')).toBeNull();
  fireEvent.change(screen.getByLabelText('Aktuelles Passwort'), { target: { value: 'old password' } });
  fireEvent.change(screen.getByLabelText('Neues Passwort'), { target: { value: 'new password' } });
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Passwort ändern' })));
  expect(screen.getByText('APP')).toBeInTheDocument();
});

it('keeps children unmounted while the session check is pending and shows errors', async () => {
  let fail!: (reason: Error) => void;
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(Response.json({ authenticated: true, mustChangePassword: false, csrfToken: 'cached' }))
    .mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject; })));
  await getSession();
  render(<LanguageProvider><WebAuthGate><div>APP</div></WebAuthGate></LanguageProvider>);
  expect(screen.getByRole('status')).toHaveTextContent('Sitzung wird geprüft');
  expect(screen.queryByText('APP')).toBeNull();
  await act(async () => fail(new Error('offline')));
  expect(screen.getByRole('alert')).toHaveTextContent('offline');
  expect(screen.queryByText('APP')).toBeNull();
});
it('unmounts children on session expiry without redirect', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(Response.json({ authenticated: true, mustChangePassword: false, csrfToken: 'a' })));
  await act(async () => { render(<LanguageProvider><WebAuthGate><div>APP</div></WebAuthGate></LanguageProvider>); });
  expect(screen.getByText('APP')).toBeInTheDocument();
  act(() => authenticationFailed(401));
  expect(screen.queryByText('APP')).toBeNull();
  expect(screen.getByRole('button', { name: 'Anmelden' })).toBeInTheDocument();
});

it('logs in through the form submission that Enter in the password field triggers', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(Response.json({ authenticated: false, mustChangePassword: false, csrfToken: null }))
    .mockResolvedValueOnce(Response.json({ authenticated: true, mustChangePassword: false, csrfToken: 'a' })));
  await act(async () => { render(<LanguageProvider><WebAuthGate><div>APP</div></WebAuthGate></LanguageProvider>); });
  const field = screen.getByLabelText('Passwort');
  // Browsers submit a form on Enter only when it has a submit button; jsdom does not emulate the key itself.
  const form = field.closest('form')!;
  expect(form.querySelector('button[type="submit"]')).not.toBeNull();
  fireEvent.change(field, { target: { value: 'ein passwort' } });
  await act(async () => { fireEvent.submit(form); });
  expect(screen.getByText('APP')).toBeInTheDocument();
});

function Switch() {
  const { setLanguage } = useLanguage();
  return <button onClick={() => setLanguage('en')}>to-english</button>;
}

it('shows a coded login error in the chosen language and changes it with the language', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(Response.json({ authenticated: false, mustChangePassword: false, csrfToken: null }))
    .mockResolvedValueOnce(Response.json({ message: 'folderNameForbiddenChar', expected: true, params: ['*'] }, { status: 422 })));
  await act(async () => { render(<LanguageProvider><Switch /><WebAuthGate><div>APP</div></WebAuthGate></LanguageProvider>); });
  fireEvent.change(screen.getByLabelText('Passwort'), { target: { value: 'wrong password' } });
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Anmelden' })));
  // A notice about an earlier session can stand beside the error, so look for the error by its text.
  const alerts = () => screen.getAllByRole('alert').map(alert => alert.textContent);
  expect(alerts()).toContain(fillParams(de.errFolderNameForbiddenChar, ['*']));
  await act(async () => screen.getByText('to-english').click());
  expect(alerts()).toContain(fillParams(en.errFolderNameForbiddenChar, ['*']));
  expect(alerts()).not.toContain(fillParams(de.errFolderNameForbiddenChar, ['*']));
  expect(screen.queryByText('APP')).toBeNull();
});

it('still shows technical text of an unexpected login error as it is', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(Response.json({ authenticated: false, mustChangePassword: false, csrfToken: null }))
    .mockResolvedValueOnce(Response.json({ message: 'Passwort nicht korrekt', expected: true }, { status: 422 })));
  await act(async () => { render(<LanguageProvider><WebAuthGate><div>APP</div></WebAuthGate></LanguageProvider>); });
  fireEvent.change(screen.getByLabelText('Passwort'), { target: { value: 'wrong password' } });
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Anmelden' })));
  expect(screen.getAllByRole('alert').map(alert => alert.textContent)).toContain('Passwort nicht korrekt');
});
