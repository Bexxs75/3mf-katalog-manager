import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { TagRemoveButton } from './TagRemoveButton';

function rect(left: number, width: number): DOMRect {
  return { left, right: left + width, width, top: 0, bottom: 20, height: 20, x: left, y: 0, toJSON: () => ({}) } as DOMRect;
}

function openMenu(buttonLeft: number) {
  // Scrolling panel starts at x=300; the 250px menu is right-aligned to the 20px wide button.
  const spy = vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (this: Element) {
    if (this.getAttribute('data-panel') !== null) return rect(300, 400);
    if (this.getAttribute('role') === null && this.hasAttribute('data-navigation-menu')) return rect(0, 250);
    if (this.tagName === 'SPAN') return rect(buttonLeft, 20);
    return rect(0, 0);
  });
  render(
    <LanguageProvider>
      <div data-panel style={{ overflowX: 'auto' }}>
        <TagRemoveButton tag="Drache" autoTag onRemove={() => {}} onRemoveAndBlock={() => {}} />
      </div>
    </LanguageProvider>,
  );
  fireEvent.click(screen.getByRole('button'));
  return spy;
}

describe('TagRemoveButton menu placement', () => {
  afterEach(() => { vi.restoreAllMocks(); });

  it('keeps the menu inside the panel when a right-aligned menu would leave it on the left', () => {
    openMenu(310);
    const menu = screen.getByRole('menu').parentElement!;
    expect(menu.style.left).toBe('-6px');
  });

  it('stays right-aligned with the button when there is room on both sides', () => {
    openMenu(600);
    const menu = screen.getByRole('menu').parentElement!;
    expect(menu.style.left).toBe('-230px');
  });

  it('pulls the menu back in when the button sits at the far right of the panel', () => {
    openMenu(690);
    const menu = screen.getByRole('menu').parentElement!;
    // Panel spans 300..700; the menu may end at 696 at most.
    expect(menu.style.left).toBe('-244px');
  });
});
