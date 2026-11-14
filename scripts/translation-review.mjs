// Parse literals only: review generation must never execute dictionary code.
import ts from 'typescript';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

export function readDictionary(source) {
  const ast = ts.createSourceFile('dictionary.ts', source, ts.ScriptTarget.Latest, true);
  const declaration = ast.statements.filter(ts.isVariableStatement).flatMap(s => [...s.declarationList.declarations])[0];
  if (!declaration || !ts.isObjectLiteralExpression(declaration.initializer)) throw Error('Expected dictionary literal');
  const result = {};
  function visit(object, prefix = '') {
    for (const p of object.properties) {
      if (!ts.isPropertyAssignment(p) || !p.name || !(ts.isIdentifier(p.name) || ts.isStringLiteral(p.name))) throw Error('Expected literal property');
      const key = prefix + p.name.text;
      if (ts.isObjectLiteralExpression(p.initializer)) visit(p.initializer, key + '.');
      else if (ts.isStringLiteral(p.initializer) || ts.isNoSubstitutionTemplateLiteral(p.initializer)) result[key] = p.initializer.text;
      else throw Error(`Non-literal translation: ${key}`);
    }
  }
  visit(declaration.initializer);
  return result;
}
export function findUsage(root) {
  const usage = new Map();
  function walk(folder) {
    for (const entry of readdirSync(folder, {withFileTypes:true})) {
      const file = path.join(folder, entry.name);
      if (entry.isDirectory()) { if (entry.name !== 'i18n') walk(file); continue; }
      if (!/\.tsx?$/.test(file) || /\.test\./.test(file)) continue;
      const source = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
      function visit(node) {
        if (ts.isStringLiteral(node)) {
          const line = source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1;
          const places = usage.get(node.text) ?? new Set();
          places.add(`${path.relative(root, file)}:${line}`);
          usage.set(node.text, places);
        }
        ts.forEachChild(node, visit);
      }
      visit(source);
    }
  }
  walk(path.join(root, 'src'));
  return usage;
}
export function reviewCsv(dictionaries, tags, usage = new Map()) {
  const languages = ['de', 'en', 'es', 'fr'];
  const rows = [['key', 'Deutsch', 'English', 'Español', 'Français', 'usage']];
  const keys = [...new Set(languages.flatMap(l => Object.keys(dictionaries[l])))].sort();
  for (const key of keys) {
    const values = languages.map(l => {
      if (typeof dictionaries[l][key] !== 'string') throw Error(`Missing ${l}:${key}`);
      return dictionaries[l][key];
    });
    rows.push([key, ...values, [...(usage.get(key.split('.')[0]) ?? ['src/i18n/*.ts (dynamic key; inspect call site)'])].sort().join('; ')]);
  }
  for (const key of Object.keys(tags).sort()) rows.push([`autoTags.${key}`, ...languages.map(l => tags[key][l]), 'src/lib/autoTags.json; tagLabel / canonical_tag']);
  return rows.map(row => row.map(value => `"${String(value).replaceAll('"', '""')}"`).join(',')).join('\r\n') + '\r\n';
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const dictionaries = Object.fromEntries(['de','en','es','fr'].map(l => [l, readDictionary(readFileSync(path.join(root, `src/i18n/${l}.ts`), 'utf8'))]));
  process.stdout.write(reviewCsv(dictionaries, JSON.parse(readFileSync(path.join(root, 'src/lib/autoTags.json'), 'utf8')), findUsage(root)));
}
