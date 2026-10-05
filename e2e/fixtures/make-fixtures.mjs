// Deterministic test models for the E2E suite. Nothing generated here is
// checked in: the same bytes come out every run, so tests can rely on sizes,
// names and plate counts. Run directly (`node make-fixtures.mjs <dir>`) to
// look at the files, or import buildFixtures() from the harness.
import { crc32, deflateSync } from 'node:zlib';
import { copyFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
// Already in the repo (53 KB, three plates), so it is not duplicated here.
const THREE_PLATES = join(here, '..', '..', 'src-tauri', 'tests', 'fixtures', 'creality-3plates.3mf');

// ---- ASCII STL ------------------------------------------------------------
function facet([a, b, c]) {
  const u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
  const v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
  const n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
  const len = Math.hypot(...n) || 1;
  const f = (x) => x.toFixed(5);
  return `facet normal ${f(n[0] / len)} ${f(n[1] / len)} ${f(n[2] / len)}\n outer loop\n` +
    [a, b, c].map((p) => `  vertex ${f(p[0])} ${f(p[1])} ${f(p[2])}\n`).join('') + ' endloop\nendfacet\n';
}
const stl = (name, tris) => `solid ${name}\n${tris.map(facet).join('')}endsolid ${name}\n`;

function boxTriangles(w, d, h) {
  const p = (x, y, z) => [x * w, y * d, z * h];
  const q = [p(0, 0, 0), p(1, 0, 0), p(1, 1, 0), p(0, 1, 0), p(0, 0, 1), p(1, 0, 1), p(1, 1, 1), p(0, 1, 1)];
  const quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
  return quads.flatMap(([a, b, c, e]) => [[q[a], q[b], q[c]], [q[a], q[c], q[e]]]);
}

// Circular base with an optional smaller top circle (radius 0 = cone).
function frustumTriangles(r1, r2, h, n = 24) {
  const ring = (r, z) => Array.from({ length: n }, (_, i) => [r * Math.cos((2 * Math.PI * i) / n), r * Math.sin((2 * Math.PI * i) / n), z]);
  const lo = ring(r1, 0), hi = ring(r2, h), tris = [];
  for (let i = 0; i < n; i++) {
    const j = (i + 1) % n;
    tris.push([lo[i], lo[j], hi[j]], [lo[i], hi[j], hi[i]]);
    tris.push([[0, 0, 0], lo[j], lo[i]]);
    if (r2 > 0) tris.push([[0, 0, h], hi[i], hi[j]]);
  }
  return tris;
}

function pyramidTriangles(s, h) {
  const a = [0, 0, 0], b = [s, 0, 0], c = [s, s, 0], d = [0, s, 0], t = [s / 2, s / 2, h];
  return [[a, c, b], [a, d, c], [a, b, t], [b, c, t], [c, d, t], [d, a, t]];
}

function prismTriangles(s, h) {
  const a = [0, 0, 0], b = [s, 0, 0], c = [s / 2, s * 0.866, 0];
  const a2 = [0, 0, h], b2 = [s, 0, h], c2 = [s / 2, s * 0.866, h];
  return [[a, c, b], [a2, b2, c2], [a, b, b2], [a, b2, a2], [b, c, c2], [b, c2, b2], [c, a, a2], [c, a2, c2]];
}

// ---- minimal ZIP (stored, no compression) ----------------------------------
// Enough for 3MF packages and the archive fixture; avoids a dependency and
// keeps the output byte-identical between runs (fixed DOS timestamp).
export function zipStore(entries) {
  const parts = [], central = [];
  let offset = 0;
  for (const [name, data] of entries) {
    const nameBuf = Buffer.from(name, 'utf8');
    const body = Buffer.isBuffer(data) ? data : Buffer.from(data, 'utf8');
    const crc = crc32(body);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0); local.writeUInt16LE(20, 4); local.writeUInt16LE(0x0800, 6);
    local.writeUInt16LE(0, 8); local.writeUInt16LE(0, 10); local.writeUInt16LE(0x21, 12);
    local.writeUInt32LE(crc, 14); local.writeUInt32LE(body.length, 18); local.writeUInt32LE(body.length, 22);
    local.writeUInt16LE(nameBuf.length, 26);
    parts.push(local, nameBuf, body);
    const cen = Buffer.alloc(46);
    cen.writeUInt32LE(0x02014b50, 0); cen.writeUInt16LE(20, 4); cen.writeUInt16LE(20, 6); cen.writeUInt16LE(0x0800, 8);
    cen.writeUInt16LE(0, 10); cen.writeUInt16LE(0, 12); cen.writeUInt16LE(0x21, 14);
    cen.writeUInt32LE(crc, 16); cen.writeUInt32LE(body.length, 20); cen.writeUInt32LE(body.length, 24);
    cen.writeUInt16LE(nameBuf.length, 28); cen.writeUInt32LE(offset, 42);
    central.push(cen, nameBuf);
    offset += 30 + nameBuf.length + body.length;
  }
  const cd = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(cd.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, cd, end]);
}

// ---- PNG -----------------------------------------------------------------------
/** Flat-colour PNG with a diagonal stripe; used as an embedded 3MF thumbnail (shows the Bild/3D toggle). */
export function stripedPng(width = 256, height = 192, rgb = [200, 120, 60]) {
  const raw = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y++) {
    const row = y * (width * 3 + 1);
    for (let x = 0; x < width; x++) {
      const stripe = ((x + y) >> 4) & 1;
      for (let c = 0; c < 3; c++) raw[row + 1 + x * 3 + c] = stripe ? rgb[c] : 235;
    }
  }
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const out = Buffer.alloc(8 + data.length + 4);
    out.writeUInt32BE(data.length, 0); body.copy(out, 4); out.writeUInt32BE(crc32(body), 8 + data.length);
    return out;
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0); ihdr.writeUInt32BE(height, 4); ihdr[8] = 8; ihdr[9] = 2;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

// ---- 3MF with two base materials (red/blue halves) -------------------------
function coloredThreeMf() {
  const verts = [[0, 0, 0], [20, 0, 0], [20, 20, 0], [0, 20, 0], [0, 0, 20], [20, 0, 20], [20, 20, 20], [0, 20, 20]];
  const quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
  const tris = quads.flatMap(([a, b, c, d], i) => [[a, b, c, i % 2], [a, c, d, i % 2]]);
  const model = `<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <metadata name="Title">Farbwuerfel</metadata>
 <resources>
  <basematerials id="1"><base name="Rot" displaycolor="#CC2222FF"/><base name="Blau" displaycolor="#2244CCFF"/></basematerials>
  <object id="2" type="model" pid="1" pindex="0"><mesh>
   <vertices>${verts.map((v) => `<vertex x="${v[0]}" y="${v[1]}" z="${v[2]}"/>`).join('')}</vertices>
   <triangles>${tris.map(([a, b, c, m]) => `<triangle v1="${a}" v2="${b}" v3="${c}" pid="1" p1="${m}"/>`).join('')}</triangles>
  </mesh></object>
 </resources>
 <build><item objectid="2"/></build>
</model>`;
  return zipStore([
    ['[Content_Types].xml', '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>'],
    ['_rels/.rels', '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>'],
    ['3D/3dmodel.model', model],
  ]);
}

/**
 * Catalog layout used by the tests. `files` are relative to the catalog
 * root; `emptyFolders` are real directories without models (drop targets).
 */
export const CATALOG = {
  folders: ['Deko', 'Haushalt', 'Technik', 'Werkstatt', 'Ablage'],
  emptyFolders: ['Ablage'],
  files: [
    { path: 'Deko/Würfel 20mm.stl', type: 'stl', dims: [20, 20, 20] },
    { path: 'Deko/Zylinder.stl', type: 'stl', dims: [20, 20, 30] },
    { path: 'Haushalt/Kegel.stl', type: 'stl', dims: [24, 24, 25] },
    { path: 'Haushalt/Dreiplatten.3mf', type: '3mf', plates: 3 },
    { path: 'Technik/Farbwuerfel.3mf', type: '3mf', dims: [20, 20, 20] },
    { path: 'Technik/Pyramide.stl', type: 'stl', dims: [30, 30, 20] },
    { path: 'Werkstatt/Prisma.stl', type: 'stl', dims: [25, 21.65, 15] },
    { path: 'Werkstatt/Platte 40mm.stl', type: 'stl', dims: [40, 40, 3] },
  ],
};

/** Writes the catalog models below `catalogDir` and extras (archive, broken file) below `extrasDir`. */
export function buildFixtures(catalogDir, extrasDir) {
  const put = (dir, rel, data) => {
    const target = join(dir, rel);
    mkdirSync(dirname(target), { recursive: true });
    if (typeof data === 'function') data(target); else writeFileSync(target, data);
  };
  for (const folder of CATALOG.folders) mkdirSync(join(catalogDir, folder), { recursive: true });
  const models = {
    'Deko/Würfel 20mm.stl': stl('wuerfel', boxTriangles(20, 20, 20)),
    'Deko/Zylinder.stl': stl('zylinder', frustumTriangles(10, 10, 30)),
    'Haushalt/Kegel.stl': stl('kegel', frustumTriangles(12, 0, 25)),
    'Haushalt/Dreiplatten.3mf': (target) => copyFileSync(THREE_PLATES, target),
    'Technik/Farbwuerfel.3mf': coloredThreeMf(),
    'Technik/Pyramide.stl': stl('pyramide', pyramidTriangles(30, 20)),
    'Werkstatt/Prisma.stl': stl('prisma', prismTriangles(25, 15)),
    'Werkstatt/Platte 40mm.stl': stl('platte', boxTriangles(40, 40, 3)),
  };
  for (const [rel, data] of Object.entries(models)) put(catalogDir, rel, data);
  if (extrasDir) {
    put(extrasDir, 'Archiv zwei Modelle.zip', zipStore([
      ['Archiv/Quader.stl', stl('quader', boxTriangles(15, 10, 5))],
      ['Archiv/Turm.stl', stl('turm', frustumTriangles(5, 5, 40))],
    ]));
    // Binary garbage with a model extension: must be reported, not crash the import.
    put(extrasDir, 'kaputt.stl', Buffer.from('solid kaputt\nfacet normal 0 0 1\n outer loop\n  vertex 0 0\n'));
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url) && process.argv[2]) {
  buildFixtures(join(process.argv[2], 'Katalog'), join(process.argv[2], 'Extras'));
  console.log(`fixtures written to ${process.argv[2]}`);
}
