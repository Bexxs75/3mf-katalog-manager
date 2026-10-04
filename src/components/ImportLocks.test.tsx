import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { CatalogResetSection } from './CatalogResetSection';
import { TrashView } from './TrashView';
import { makeModelFile } from '../test/factories';
vi.mock('@tauri-apps/api/core', () => ({invoke:vi.fn().mockResolvedValue(false)}));
beforeEach(() => localStorage.clear());
it('locks a reset confirmation that was already open when import started', () => {
  const ui = (active:boolean) => <LanguageProvider><ImportLockContext.Provider value={active}><CatalogResetSection modelCount={2} folderCount={1} onExport={vi.fn()} onReset={vi.fn()} /></ImportLockContext.Provider></LanguageProvider>;
  const {rerender}=render(ui(false)); fireEvent.click(screen.getByRole('button',{name:/Katalog zurücksetzen/}));
  rerender(ui(true));
  const buttons=screen.getAllByRole('button').filter(b => b.textContent?.includes('zurücksetzen'));
  expect(buttons.length).toBeGreaterThan(0);
  for (const button of buttons) {expect(button).toBeDisabled();expect(button).toHaveAttribute('title','Während eines Imports gesperrt');}
});
it('locks emptying, restoring and permanent deletion in the trash', () => {
  const model=makeModelFile({id:'m1',deletedAt:'2026-10-04T10:00:00Z'});
  render(<LanguageProvider><UiDensityProvider><ImportLockContext.Provider value={true}><TrashView trashModels={[model]} selectedId="m1" onSelect={vi.fn()} view="grid" confirmEmptyTrash={false} onConfirmEmptyTrashChange={vi.fn()} onEmptyTrash={vi.fn()} onRestore={vi.fn()} onDeletePermanently={vi.fn()} displayPreference="thumbnail" /></ImportLockContext.Provider></UiDensityProvider></LanguageProvider>);
  for (const name of [/Papierkorb leeren/, /Wiederherstellen/, /Endgültig löschen/]) {
    const button=screen.getByRole('button',{name});expect(button).toBeDisabled();expect(button).toHaveAttribute('title','Während eines Imports gesperrt');
  }
});
