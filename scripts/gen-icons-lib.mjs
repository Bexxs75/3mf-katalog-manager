const required = {
  viewBox: '0 0 24 24',
  'stroke-width': '1.8',
  fill: 'none',
  stroke: 'currentColor',
};

export function generateIcons(sources) {
  const entries = Object.keys(sources).sort().map((file) => {
    const source = sources[file];
    const svg = source.match(/^\s*<svg\b([^>]*)>([\s\S]*?)<\/svg>\s*$/);
    if (!svg) throw new Error(`${file}: gültiges SVG-Wurzelelement erwartet`);
    const attributes = Object.fromEntries(
      [...svg[1].matchAll(/([\w:-]+)\s*=\s*(["'])(.*?)\2/g)].map((match) => [match[1], match[3]]),
    );
    for (const [attribute, expected] of Object.entries(required)) {
      if (attributes[attribute] !== expected) {
        throw new Error(`${file}: ${attribute} muss "${expected}" sein (gefunden: ${JSON.stringify(attributes[attribute])})`);
      }
    }
    return `  ${JSON.stringify(file.slice(0, -4))}: ${JSON.stringify(svg[2].replace(/<title\b[^>]*>[\s\S]*?<\/title>/g, ''))},`;
  });
  return [
    '// generiert, nicht von Hand ändern — npm run gen:icons',
    "import type { IconName } from './Icon';",
    '',
    'export const icons: Record<IconName, string> = {',
    ...entries,
    '};',
    '',
  ].join('\n');
}
