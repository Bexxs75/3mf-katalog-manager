import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { WebAccountMenu, WEB_CHANGE_PASSWORD_EVENT } from './WebAccountMenu';
import * as session from '../lib/webSession';

afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const mount = () => render(<LanguageProvider><WebAccountMenu /></LanguageProvider>);

it('keeps password change and logout inside a menu until it is opened', () => {
  mount();
  expect(screen.queryByRole('menuitem', { name: 'Abmelden' })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Konto' }));
  expect(screen.getByRole('menuitem', { name: 'Passwort ändern' })).toBeVisible();
  expect(screen.getByRole('menuitem', { name: 'Abmelden' })).toBeVisible();
});

it('asks the gate to open the password form and closes', () => {
  const seen = vi.fn(); window.addEventListener(WEB_CHANGE_PASSWORD_EVENT, seen);
  mount();
  fireEvent.click(screen.getByRole('button', { name: 'Konto' }));
  fireEvent.click(screen.getByRole('menuitem', { name: 'Passwort ändern' }));
  expect(seen).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole('menu')).toBeNull();
  window.removeEventListener(WEB_CHANGE_PASSWORD_EVENT, seen);
});

it('logs out through the session and shows a failure instead of hiding it', async () => {
  const out = vi.spyOn(session, 'logout').mockRejectedValueOnce(new Error('Server nicht erreichbar'));
  mount();
  fireEvent.click(screen.getByRole('button', { name: 'Konto' }));
  fireEvent.click(screen.getByRole('menuitem', { name: 'Abmelden' }));
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Server nicht erreichbar'));
  expect(out).toHaveBeenCalledTimes(1);
  expect(screen.getByRole('menu')).toBeVisible();
});

it('closes on Escape and on a click outside', () => {
  mount();
  fireEvent.click(screen.getByRole('button', { name: 'Konto' }));
  fireEvent.keyDown(document, { key: 'Escape' });
  expect(screen.queryByRole('menu')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Konto' }));
  fireEvent.mouseDown(document.body);
  expect(screen.queryByRole('menu')).toBeNull();
});
