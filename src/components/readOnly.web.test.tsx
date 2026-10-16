import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
vi.mock('../lib/transport', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
import { ContextMenu } from './ContextMenu';
import { ViewerErrorCard } from './ViewerErrorCard';
import { KeyboardTipsDialog, EmptyCatalogTips } from './KeyboardTipsDialog';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { makeModelFile } from '../test/factories';
import { de } from '../i18n/de';
import { invoke } from '../lib/transport';
beforeEach(() => { vi.stubEnv('VITE_TARGET', 'web'); localStorage.clear(); });
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.unstubAllEnvs(); });
it.each(['model', 'collection'])('does not mount %s mutation context menus even if requested directly', kind => {
  const action = vi.fn();
  const common = { x: 0, y: 0, onClose: action };
  render(<LanguageProviderWithDiagnostics>{kind === 'collection'
    ? <ContextMenu {...common} collectionActions={{ onRename: action, onDelete: action }} />
    : <ContextMenu {...common} fileId="1" currentName="model.stl" inQueue={false} printed={false}
      onOpenInSlicer={action} onDelete={action} onToggleQueue={action} onTogglePrintStatus={action} onRename={action} />}
  </LanguageProviderWithDiagnostics>);
  expect(screen.queryByRole('button')).toBeNull(); expect(invoke).not.toHaveBeenCalled(); expect(action).not.toHaveBeenCalled();
});
it.each(['notFound', 'unsupported'] as const)('hides native and removal actions from %s preview errors', code => {
  const action = vi.fn();
  render(<LanguageProviderWithDiagnostics><ViewerErrorCard error={{ message: 'Preview failed', unexpected: false, code }}
    model={makeModelFile()} noWebGL={false} compact={false} onOpenInSlicer={action} onRemoveFromCatalog={action} />
  </LanguageProviderWithDiagnostics>);
  expect(screen.getByRole('alert')).toBeInTheDocument(); expect(screen.queryByRole('button')).toBeNull();
  expect(action).not.toHaveBeenCalled();
});
it('omits write shortcuts and drag/bulk/context tips but retains navigation help', () => {
  render(<LanguageProviderWithDiagnostics><KeyboardTipsDialog onClose={vi.fn()} /></LanguageProviderWithDiagnostics>);
  for (const text of [de.keyboardToggle, de.keyboardSelectAll, de.keyboardDelete]) expect(screen.queryByText(text)).toBeNull();
  expect(screen.getByText(de.keyboardDetailNavigate)).toBeInTheDocument();
  fireEvent.click(screen.getByRole('tab', { name: de.tipsTab }));
  for (const text of [de.tipDragTitle, de.tipBulkTitle, de.tipContextTitle]) expect(screen.queryByText(text)).toBeNull();
  expect(screen.getByText(de.tipSortTitle)).toBeInTheDocument();
});
it('does not suggest importing or editing in an empty web catalog', () => {
  render(<LanguageProviderWithDiagnostics><EmptyCatalogTips onOpenTips={vi.fn()} /></LanguageProviderWithDiagnostics>);
  expect(screen.getByText(de.webReadOnly)).toBeInTheDocument(); expect(screen.queryByText(de.emptyContextTitle)).toBeNull();
});
