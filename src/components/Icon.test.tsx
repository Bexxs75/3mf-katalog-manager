import generatedSource from './icons.generated.ts?raw';
import { generateIcons } from '../../scripts/gen-icons-lib.mjs';
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { Icon, type IconName } from './Icon';
import { icons } from './icons.generated';

const sourceFiles = import.meta.glob<string>('../../docs/assets/icons/*.svg', {
  query: '?raw', import: 'default', eager: true,
});
const sources = Object.fromEntries(Object.entries(sourceFiles).map(([path, source]) => [path.split('/').pop()!, source]));
const names = Object.keys(sources).map((file) => file.slice(0, -4)) as IconName[];

describe('Icon', () => {
  it('covers exactly the 36 approved source names', () => {
    expect(names).toHaveLength(36);
    expect(Object.keys(icons).sort()).toEqual([...names].sort());
  });

  it.each(names)('renders %s with the approved decorative SVG attributes and geometry', (name) => {
    const { container } = render(<Icon name={name} />);
    const svg = container.querySelector('svg')!;
    for (const [attribute, value] of Object.entries({
      viewBox: '0 0 24 24', width: '24', height: '24', fill: 'none',
      stroke: 'currentColor', 'stroke-width': '1.8', 'stroke-linecap': 'round',
      'stroke-linejoin': 'round', 'aria-hidden': 'true', focusable: 'false',
    })) {
      expect(svg).toHaveAttribute(attribute, value);
    }
    expect(svg.querySelector('title')).toBeNull();
    const source = new DOMParser().parseFromString(
      sources[`${name}.svg`], 'image/svg+xml',
    ).documentElement;
    source.querySelectorAll('title').forEach((title) => title.remove());
    expect(svg.children).toHaveLength(source.children.length);
    [...svg.children].forEach((child, index) => {
      expect(child.isEqualNode(source.children[index])).toBe(true);
    });
  });

  it('accepts a custom size and className', () => {
    const { container } = render(<Icon name="catalog" size={19} className="custom-icon" />);
    expect(container.firstChild).toHaveAttribute('width', '19');
    expect(container.firstChild).toHaveAttribute('height', '19');
    expect(container.firstChild).toHaveClass('custom-icon');
  });

  it.each(['viewBox', 'stroke-width', 'fill', 'stroke'])(
    'rejects an invalid root %s with the source filename', (attribute) => {
      const invalid = sources['catalog.svg']
        .replace(new RegExp(`${attribute}="[^"]*"`), `${attribute}="invalid"`);
      expect(() => generateIcons({ 'catalog.svg': invalid }))
        .toThrow(`catalog.svg: ${attribute} muss`);
    },
  );

  it('matches fresh generator output without rewriting the checked-in file', () => {
    const generated = generateIcons(sources);
    expect(generatedSource).toBe(generated);
  });
});
