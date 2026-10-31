import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { WebAuthGate } from './WebAuthGate';
import { authenticationFailed, getSession } from '../lib/webSession';
afterEach(() => vi.unstubAllGlobals());
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
