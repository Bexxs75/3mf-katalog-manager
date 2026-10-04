import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { generateIcons } from './gen-icons-lib.mjs';

try {
  const sourceDir = new URL('../docs/assets/icons/', import.meta.url);
  const sources = Object.fromEntries(
    readdirSync(sourceDir).filter((file) => file.endsWith('.svg'))
      .map((file) => [file, readFileSync(new URL(file, sourceDir), 'utf8')]),
  );
  writeFileSync(new URL('../src/components/icons.generated.ts', import.meta.url), generateIcons(sources));
} catch (error) {
  console.error(`Icon-Generierung fehlgeschlagen: ${error.message}`);
  process.exitCode = 1;
}
