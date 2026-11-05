import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
vi.mock('../lib/transport', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
import { ContextMenu } from './ContextMenu';
import { BundleContextMenu } from './BundleContextMenu';
import { BulkActionToolbar } from './BulkActionToolbar';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { de } from '../i18n/de';
import { login, authenticationFailed } from '../lib/webSession';

beforeEach(() => { vi.stubEnv('VITE_TARGET', 'web'); localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.clearAllMocks(); vi.unstubAllEnvs(); });

const noop = () => {};
const bundleActions = { onCreate: noop, onAddTo: noop, onRemove: noop };
const fileMenu = () => render(<LanguageProviderWithDiagnostics>
  <ContextMenu x={0} y={0} onClose={noop} fileId="1" currentName="model.stl" inQueue={false} printed={false} onOpenInSlicer={noop}
    onDelete={noop} onToggleQueue={noop} onTogglePrintStatus={noop} onRename={async () => {}} bundleActions={bundleActions} />
</LanguageProviderWithDiagnostics>);

it('offers no bundle entry in the read-only web version', () => {
  fileMenu();
  for (const label of [de.bundleMake, de.bundleAddTo, de.bundleRemoveFrom]) expect(screen.queryByRole('button', { name: label })).toBeNull();
});

it('offers the file menu entries once the session may edit metadata', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ authenticated: true, writable: true, csrfToken: 'csrf' })));
  await login('test');
  fileMenu();
  for (const label of [de.bundleMake, de.bundleAddTo, de.bundleRemoveFrom]) expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
});

it('mounts neither the bundle card menu nor the selection bar without the right to edit', () => {
  const action = vi.fn();
  const { container } = render(<LanguageProviderWithDiagnostics>
    <BundleContextMenu x={0} y={0} count={2} allPrinted={false} allFavorite={false} canQueue canDelete onClose={action} onEdit={action}
      onTogglePrinted={action} onQueueAll={action} onToggleFavorite={action} onDissolve={action} onDelete={action} />
    <BulkActionToolbar selectedCount={2} confirmBulkDelete={false} onConfirmBulkDeleteChange={action} onSelectAllVisible={action} onClearSelection={action}
      onBulkAddToQueue={action} addToCollectionMenuOpen={false} onAddToCollectionMenuOpenChange={action} collections={[]} onBulkAddToCollection={action}
      activeCollection={null} onBulkRemoveFromCollection={action} onBulkSetPrintStatus={action} onBulkDelete={action} addTagMenuOpen={false}
      onAddTagMenuOpenChange={action} allTags={[]} tagsInAllSelection={[]} tagDraft="" onTagDraftChange={action} onSubmitBulkAddTag={action}
      removeTagMenuOpen={false} onRemoveTagMenuOpenChange={action} tagsInSelection={[]} onBulkRemoveTag={action}
      bundleActions={{ canCreate: true, canAddTo: true, canRemove: true, canDissolve: true, canEdit: true, onCreate: action, onAddTo: action, onRemove: action, onDissolve: action, onEdit: action }} />
  </LanguageProviderWithDiagnostics>);
  expect(container).toBeEmptyDOMElement();
  expect(action).not.toHaveBeenCalled();
});
