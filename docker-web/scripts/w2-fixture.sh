#!/usr/bin/env bash
# Throw-away model folders (invented content, no user data) for the W2 browser test (w2-browser-test.mjs).
# Usage:
#   w2-fixture.sh base   DIR   two folders below DIR: alpha (30 models) and beta (6 models)
#   w2-fixture.sh bulk|late DIR   25 / 3 more new models (cancel test / start check)
#   w2-fixture.sh mutate DIR   changes them AFTER the catalog has taken the base state over:
#       alpha: 3 single models deleted, folders "Ordner-weg" (4 models) and "Archiv-alt" (3 models) deleted as a whole,
#              5 new models, folder "Sub" replaced by a plain file and folder "Verlinkt" by a
#              symlink (both stay "unclear": the server cannot tell whether the models are gone)
#       beta:  1 model deleted, 1 new model
# The 3MF files are minimal valid packages (one tetrahedron, size differs per file); two files in alpha
# are byte-identical copies (duplicates). Needs python3 only.
set -euo pipefail
mode=${1:?usage: w2-fixture.sh base|mutate|bulk|late DIR}
dir=${2:?usage: w2-fixture.sh base|mutate|bulk|late DIR}

make_model() { # path scale  -> writes a minimal 3MF
    python3 - "$1" "$2" <<'PY'
import sys, zipfile
path, scale = sys.argv[1], float(sys.argv[2])
s = 10 * scale
model = f'''<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <metadata name="Title">Testmodell {scale}</metadata>
 <resources><object id="1" type="model"><mesh>
  <vertices><vertex x="0" y="0" z="0"/><vertex x="{s}" y="0" z="0"/><vertex x="0" y="{s}" z="0"/><vertex x="0" y="0" z="{s}"/></vertices>
  <triangles><triangle v1="0" v2="2" v3="1"/><triangle v1="0" v2="1" v3="3"/><triangle v1="1" v2="2" v3="3"/><triangle v1="2" v2="0" v3="3"/></triangles>
 </mesh></object></resources>
 <build><item objectid="1"/></build>
</model>'''
types = '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>'
rels = '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>'
with zipfile.ZipFile(path, 'w', zipfile.ZIP_STORED) as z:
    for name, data in (('[Content_Types].xml', types), ('_rels/.rels', rels), ('3D/3dmodel.model', model)):
        info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))  # fixed time: byte-identical for equal input
        z.writestr(info, data)
PY
}
put() { mkdir -p "$(dirname "$1")"; make_model "$1" "$2"; }

case "$mode" in
base)
    rm -rf "$dir/alpha" "$dir/beta"
    n=1
    for folder in Deko Haushalt Technik; do
        for i in 1 2 3 4 5; do put "$dir/alpha/$folder/Modell-0$i.3mf" "$n"; n=$((n + 1)); done
    done
    for i in 1 2; do put "$dir/alpha/Werkstatt/Modell-0$i.3mf" "$n"; n=$((n + 1)); done
    for i in 1 2 3 4; do put "$dir/alpha/Ordner-weg/Teil-$i.3mf" "$((100 + i))"; done
    for i in 1 2 3; do put "$dir/alpha/Archiv-alt/Alt-$i.3mf" "$((150 + i))"; done
    put "$dir/alpha/Sub/Einzelteil.3mf" 201
    put "$dir/alpha/Verlinkt/Linkteil.3mf" 202
    for i in 1 2; do put "$dir/alpha/Sonstiges/Rest-$i.3mf" "$((300 + i))"; done
    # Duplicates: same bytes as Deko/Modell-02, under two other names.
    cp "$dir/alpha/Deko/Modell-02.3mf" "$dir/alpha/Sonstiges/Kopie von Modell-02.3mf"
    cp "$dir/alpha/Deko/Modell-02.3mf" "$dir/alpha/Technik/Modell-02 (Kopie).3mf"
    for i in 1 2 3 4 5 6; do put "$dir/beta/Beta-$i.3mf" "$((400 + i))"; done
    echo "alpha: $(find "$dir/alpha" -name '*.3mf' | wc -l) models, beta: $(find "$dir/beta" -name '*.3mf' | wc -l) models"
    ;;
mutate)
    rm -f "$dir/alpha/Deko/Modell-01.3mf" "$dir/alpha/Haushalt/Modell-01.3mf" "$dir/alpha/Technik/Modell-01.3mf"
    rm -rf "$dir/alpha/Ordner-weg" "$dir/alpha/Archiv-alt"
    for i in 1 2 3 4; do put "$dir/alpha/Neu/Neu-$i.3mf" "$((500 + i))"; done
    put "$dir/alpha/Deko/Neu-5.3mf" 505
    rm -rf "$dir/alpha/Sub";      printf 'ich bin jetzt eine Datei' > "$dir/alpha/Sub"
    rm -rf "$dir/alpha/Verlinkt"; ln -s Deko "$dir/alpha/Verlinkt"
    rm -f "$dir/beta/Beta-1.3mf"
    put "$dir/beta/Beta-neu.3mf" 506
    echo "mutated: alpha -3 single, -4 folder, +5 new, 2 unclear; beta -1, +1"
    ;;
bulk)   # 25 more new models (more than two blocks of 10 rows, for the cancel test)
    for i in $(seq -w 1 25); do put "$dir/alpha/Massen/M-$i.3mf" "$((600 + 10#$i))"; done
    echo "bulk: +25 new in alpha/Massen"
    ;;
late)   # 3 new models that only the next server start sees (start check)
    for i in 1 2 3; do put "$dir/alpha/Spaet/Spaet-$i.3mf" "$((700 + i))"; done
    echo "late: +3 new in alpha/Spaet"
    ;;
*) echo "unknown mode $mode" >&2; exit 2;;
esac
