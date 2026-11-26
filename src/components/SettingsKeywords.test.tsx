import { render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { SettingsSection } from './SettingsSection';
import { SettingRow, SettingsToolsContext } from './SettingsTools';

function renderWith(query: string) {
  const tools = { query, report: vi.fn(), reportSection: vi.fn() };
  render(<LanguageProviderWithDiagnostics><SettingsToolsContext.Provider value={tools}>
    <SettingsSection id="program" title="Programm">
      <SettingRow id="close" value="exit" label="Schliessen" keywords="Tray System Tray"><p>Beim Schliessen beenden</p></SettingRow>
      <SettingRow id="density" value="compact" label="Dichte" keywords="Abstand Padding"><p>Kompakt</p></SettingRow>
    </SettingsSection>
  </SettingsToolsContext.Provider></LanguageProviderWithDiagnostics>);
  return tools;
}

it.each([['Tray', 'Schliessen'], ['padding', 'Dichte']])('finds a setting by its keyword %s', (query, label) => {
  const tools = renderWith(query);
  expect(tools.reportSection).toHaveBeenLastCalledWith('program', expect.anything(), true);
  expect(screen.getByText(label === 'Schliessen' ? 'Beim Schliessen beenden' : 'Kompakt')).toBeVisible();
});

it('keeps keywords out of the visible text and the accessibility tree and still hides non-matches', () => {
  const tools = renderWith('zzz');
  expect(tools.reportSection).toHaveBeenLastCalledWith('program', expect.anything(), false);
  expect(document.body.textContent).not.toContain('System Tray');
});
