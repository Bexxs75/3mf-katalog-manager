import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Rail } from './Rail';
import { icons } from './icons.generated';


beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function renderRail(mainView: 'catalog' | 'filament' | 'printers' | 'trash' = 'catalog', trashCount = 3, controlledSettings = false, container = false) {
  const props: Parameters<typeof Rail>[0] = {
    mainView, trashCount, onMainViewChange: vi.fn(),
    settingsOpen: false, onSettingsOpenChange: vi.fn(),
  };
  function ControlledRail() {
    const [settingsOpen, setSettingsOpen] = useState(false);
    return <>
      <Rail {...props} settingsOpen={settingsOpen} onSettingsOpenChange={(open) => {
        props.onSettingsOpenChange(open);
        setSettingsOpen(open);
      }} />
      {/* Stands in for the settings panel, which lives outside the rail. */}
      {settingsOpen && <button>panel</button>}
    </>;
  }
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container }}>{controlledSettings ? <ControlledRail /> : <Rail {...props} />}</RuntimeEnvironmentProvider></LanguageProvider>);
  return props;
}

const buttons = [
  ['Katalog', 'catalog'], ['Material Manager', 'spool'],
  ['Printer Manager', 'printer'], ['Papierkorb', 'trash'], ['Einstellungen', 'settings'],
] as const;

describe('Rail', () => {
  it('keeps five named 42px buttons in order with their approved 24px icons and focus style', () => {
    renderRail();
    const rendered = within(screen.getByRole('navigation')).getAllByRole('button');
    expect(rendered).toHaveLength(5);
    buttons.forEach(([label, name], index) => {
      const button = rendered[index];
      expect(button).toHaveAttribute('aria-label', label);
      expect(button).toHaveAttribute('title', label);
      expect(button).toHaveClass('w-[42px]', 'h-[42px]', 'grid', 'place-items-center',
        'focus-visible:outline', 'focus-visible:outline-2', 'focus-visible:outline-[var(--accent)]');
      const svg = button.querySelector('svg[aria-hidden="true"]');
      expect(svg).toHaveAttribute('width', '24');
      const expected = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      expected.innerHTML = icons[name];
      expect(svg?.innerHTML).toBe(expected.innerHTML);
    });
    expect(within(rendered[3]).getByText('3')).toHaveClass('absolute', '-top-1', '-right-1');
  });

  it.each([
    ['Katalog', 'catalog'], ['Material Manager', 'filament'], ['Printer Manager', 'printers'], ['Papierkorb', 'trash'],
  ] as const)('keeps %s active and dispatches its view on click', (label, view) => {
    const props = renderRail(view);
    const button = screen.getByRole('button', { name: label });
    expect(button).toHaveClass('text-[var(--accent)]');
    for (const [other] of buttons.slice(0, 4)) {
      if (other !== label) expect(screen.getByRole('button', { name: other })).not.toHaveClass('text-[var(--accent)]');
    }
    fireEvent.click(button);
    expect(props.onMainViewChange).toHaveBeenCalledExactlyOnceWith(view);
  });

  it('opens settings on click and omits the badge for an empty trash', () => {
    const props = renderRail('catalog', 0);
    expect(screen.getByRole('button', { name: 'Papierkorb' }).querySelector('span')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Einstellungen' }));
    expect(props.onSettingsOpenChange).toHaveBeenCalledExactlyOnceWith(true);
  });
});

describe('Rail settings gear', () => {
  it('toggles the settings and reports the state to assistive technology', () => {
    const props = renderRail('catalog', 0, true);
    const gear = screen.getByRole('button', { name: 'Einstellungen' });
    expect(gear).toHaveAttribute('aria-expanded', 'false');
    fireEvent.click(gear);
    expect(gear).toHaveAttribute('aria-expanded', 'true');
    fireEvent.click(gear);
    expect(props.onSettingsOpenChange).toHaveBeenNthCalledWith(2, false);
  });

  it('hands focus back to the gear when the focused panel disappears on close', () => {
    renderRail('catalog', 0, true);
    const gear = screen.getByRole('button', { name: 'Einstellungen' });
    fireEvent.click(gear);
    screen.getByRole('button', { name: 'panel' }).focus();
    // Closing through the gear moves focus there anyway; blur it to model the cross/Escape path.
    gear.blur();
    fireEvent.click(gear);
    expect(screen.queryByRole('button', { name: 'panel' })).toBeNull();
    expect(gear).toHaveFocus();
  });

  it('does not steal focus the user already moved to another control', () => {
    renderRail('catalog', 0, true);
    const gear = screen.getByRole('button', { name: 'Einstellungen' });
    fireEvent.click(gear);
    const catalog = screen.getByRole('button', { name: 'Katalog' });
    catalog.focus();
    fireEvent.click(gear);
    expect(catalog).toHaveFocus();
  });
});

it('offers Download & Slicer only in the container and dispatches navigation', () => {
  const props = renderRail('catalog', 0, false, true);
  fireEvent.click(screen.getByRole('button', { name: 'Download & Slicer' }));
  expect(props.onMainViewChange).toHaveBeenCalledWith('downloads');
});
it('does not offer Download & Slicer on desktop', () => {
  renderRail();
  expect(screen.queryByRole('button', { name: 'Download & Slicer' })).toBeNull();
});
