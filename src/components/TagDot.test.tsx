import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { TagDot } from './TagDot';

describe('TagDot', () => {
  it('draws the dot in the stored hue and hides it from screen readers', () => {
    const { container } = render(<TagDot hue={120} />);
    const dot = container.firstElementChild as HTMLElement;
    expect(dot).toHaveAttribute('aria-hidden', 'true');
    expect(dot.style.background).toContain('120');
  });

  it('draws nothing for a tag without a known hue', () => {
    const { container } = render(<TagDot hue={undefined} />);
    expect(container).toBeEmptyDOMElement();
  });
});
