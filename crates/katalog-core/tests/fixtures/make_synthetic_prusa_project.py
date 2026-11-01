#!/usr/bin/env python3
"""Builds synthetic-prusa-project.3mf, a SYNTHETIC test package.

Everything in it is invented: neutral numbers, one cube, no designer, title,
description or URL fields. Only the file layout and the key names of a PrusaSlicer
project saved WITHOUT slicing (no G-code, no slice_info.config, no plate_N.json) are
imitated: Metadata/Slic3r_PE.config (text, one "; key = value" per line) and
Metadata/Slic3r_PE_model.config (XML). It is NOT a copy or an extract of a real file.

Usage: python3 make_synthetic_prusa_project.py synthetic-prusa-project.3mf
The output is byte-for-byte reproducible (fixed timestamps, fixed order).
"""
import sys
import zipfile

COMMENT = b"SYNTHETIC test package (structure imitated, all values invented). Not a real slicer file."

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
 <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
 <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
 <Default Extension="png" ContentType="image/png"/>
</Types>
"""

RELS = """<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
 <Relationship Target="/3D/3dmodel.model" Id="rel-1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>
"""

CUBE_VERTS = [(0, 0, 0), (10, 0, 0), (10, 10, 0), (0, 10, 0), (0, 0, 10), (10, 0, 10), (10, 10, 10), (0, 10, 10)]
CUBE_TRIS = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
             (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]

def model_xml():
    v = "\n".join(f'    <vertex x="{x}" y="{y}" z="{z}"/>' for x, y, z in CUBE_VERTS)
    t = "\n".join(f'    <triangle v1="{a}" v2="{b}" v3="{c}"/>' for a, b, c in CUBE_TRIS)
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:slic3rpe="http://schemas.slic3r.org/3mf/2017/06">
 <metadata name="slic3rpe:Version3mf">1</metadata>
 <resources>
  <object id="1" type="model">
   <mesh>
    <vertices>
{v}
    </vertices>
    <triangles>
{t}
    </triangles>
   </mesh>
  </object>
 </resources>
 <build>
  <item objectid="1" transform="1 0 0 0 1 0 0 0 1 125 105 5" printable="1"/>
 </build>
</model>
"""

MODEL_CONFIG = """<?xml version="1.0" encoding="UTF-8"?>
<config>
 <object id="1" instances_count="1">
  <metadata type="object" key="name" value="cube"/>
  <volume firstid="0" lastid="11">
   <metadata type="volume" key="name" value="cube"/>
   <metadata type="volume" key="volume_type" value="ModelPart"/>
  </volume>
 </object>
</config>
"""

WIPE_TOWER = """<?xml version="1.0" encoding="UTF-8"?>
<wipe_tower_information>
 <info x="170" y="140" width="60" depth="0" rotation_angle="0"/>
</wipe_tower_information>
"""

# Invented values with the shapes of a single-extruder project. The temperatures are
# present on purpose: the reader must not take them over for a file without print data.
CONFIG = [
    ("avoid_crossing_perimeters", "0"),
    ("bed_shape", "0x0,250x0,250x210,0x210"),
    ("bed_temperature", "70"),
    ("filament_settings_id", '"Example PETG"'),
    ("filament_type", "PETG"),
    ("first_layer_bed_temperature", "75"),
    ("first_layer_height", "0.2"),
    ("first_layer_temperature", "235"),
    ("layer_height", "0.15"),
    ("nozzle_diameter", "0.4"),
    ("print_settings_id", '"0.15mm EXAMPLE"'),
    ("printer_model", "MK4S"),
    ("printer_settings_id", '"Example Printer 0.4 nozzle"'),
    ("temperature", "230"),
]

def config_text():
    return "".join(f"; {k} = {v}\n" for k, v in CONFIG)

def main(out):
    entries = [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", RELS),
        ("3D/3dmodel.model", model_xml()),
        ("Metadata/Prusa_Slicer_wipe_tower_information.xml", WIPE_TOWER),
        ("Metadata/Slic3r_PE.config", config_text()),
        ("Metadata/Slic3r_PE_model.config", MODEL_CONFIG),
    ]
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.comment = COMMENT
        for name, content in entries:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, content)

if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "synthetic-prusa-project.3mf")
