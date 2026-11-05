import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
vi.mock('../lib/transport', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
import { WebFileActions } from './WebFileActions';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { authenticationFailed, login } from '../lib/webSession';

beforeEach(async () => {
  vi.stubEnv('VITE_TARGET', 'web');
  localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de');
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'csrf', mustChangePassword: false })));
  await login('test');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.clearAllMocks(); vi.unstubAllEnvs(); });

const trashButton = () => screen.getByRole('button', { name: 'In den Papierkorb' });

it('hands the trash request to the caller, who may ask something first, instead of opening its own confirmation', () => {
  const onTrashRequest = vi.fn();
  render(<LanguageProviderWithDiagnostics><WebFileActions fileIds={['1', '2']} names={['a', 'b']} onTrashRequest={onTrashRequest} /></LanguageProviderWithDiagnostics>);
  fireEvent.click(trashButton());
  expect(onTrashRequest).toHaveBeenCalledOnce();
  expect(screen.queryByRole('dialog')).toBeNull();
});

it('still opens its confirmation when the caller answers through requestTrashConfirm', () => {
  const props = { fileIds: ['1', '2'], names: ['a', 'b'] };
  const { rerender } = render(<LanguageProviderWithDiagnostics><WebFileActions {...props} onTrashRequest={vi.fn()} /></LanguageProviderWithDiagnostics>);
  rerender(<LanguageProviderWithDiagnostics><WebFileActions {...props} onTrashRequest={vi.fn()} requestTrashConfirm /></LanguageProviderWithDiagnostics>);
  expect(screen.getByRole('dialog')).toBeInTheDocument();
});

it('opens its confirmation directly where no caller takes the request', () => {
  render(<LanguageProviderWithDiagnostics><WebFileActions fileIds={['1']} name="a.stl" /></LanguageProviderWithDiagnostics>);
  fireEvent.click(trashButton());
  expect(screen.getByRole('dialog')).toBeInTheDocument();
});
