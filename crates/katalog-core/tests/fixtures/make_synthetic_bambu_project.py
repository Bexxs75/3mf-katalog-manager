#!/usr/bin/env python3
"""Builds synthetic-bambu-project.3mf, a SYNTHETIC test package.

Everything in it is invented: neutral numbers, one cube, no designer, title,
description or URL fields and no images. Only the file layout and the key names
of a Bambu Studio project (slice_info.config with a header only,
project_settings.config as JSON, model_settings.config with plates,
plate_N.json) are imitated. It is NOT a copy or an extract of a real file.

Usage: python3 make_synthetic_bambu_project.py synthetic-bambu-project.3mf
       python3 make_synthetic_bambu_project.py --colors synthetic-bambu-colors.3mf
The second form builds synthetic-bambu-colors.3mf: the same project plus a four-entry
`filament_colour` list and extruder entries per object and part (extruders 1 and 3 are
used, 2 and 4 are not), for the filament color reading of the print data.
The outputs are byte-for-byte reproducible (fixed timestamps, fixed order).
"""
import json
import sys
import zipfile

COMMENT = b"SYNTHETIC test package (structure imitated, all values invented). Not a real slicer file."

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
 <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
 <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
 <Default Extension="json" ContentType="application/json"/>
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
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:BambuStudio="http://schemas.bambulab.com/package/2021">
 <metadata name="Application">BambuStudio-00.00.00.00</metadata>
 <metadata name="BambuStudio:3mfVersion">1</metadata>
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
  <item objectid="1" transform="1 0 0 0 1 0 0 0 1 0 0 0" printable="1"/>
 </build>
</model>
"""

def plate_block(n):
    return f""" <plate>
  <metadata key="plater_id" value="{n}"/>
  <metadata key="plater_name" value=""/>
  <metadata key="locked" value="false"/>
 </plate>
"""

MODEL_SETTINGS = """<?xml version="1.0" encoding="UTF-8"?>
<config>
 <object id="1">
  <metadata key="name" value="cube"/>
 </object>
""" + "".join(plate_block(n) for n in (1, 2, 3, 4)) + "</config>\n"

# Header only, no <plate>: the "no print data" case.
SLICE_INFO = """<?xml version="1.0" encoding="UTF-8"?>
<config>
  <header>
    <header_item key="X-BBL-Client-Type" value="slicer"/>
    <header_item key="X-BBL-Client-Version" value="00.00.00.00"/>
  </header>
</config>
"""

# --colors variant: invented colors; entry n belongs to extruder n. Extruder 2 (red) and
# extruder 4 (yellow) are listed in the project but no object or part uses them.
COLOR_LIST = ["#2E7D32", "#FF0000FF", "#1565C0FF", "#F9A825"]

MODEL_SETTINGS_COLORS = """<?xml version="1.0" encoding="UTF-8"?>
<config>
 <object id="1">
  <metadata key="name" value="cube"/>
  <metadata key="extruder" value="1"/>
  <part id="1" subtype="normal_part">
   <metadata key="name" value="body"/>
  </part>
  <part id="2" subtype="normal_part">
   <metadata key="name" value="lid"/>
   <metadata key="extruder" value="3"/>
  </part>
 </object>
""" + "".join(plate_block(n) for n in (1, 2, 3, 4)) + "</config>\n"

def settings():
    # Invented values; lists have the shapes seen in real files (9 filaments,
    # a longer nozzle list, a trailing "0"), but the numbers are made up.
    nine = lambda *v: [str(x) for x in v]
    return {
        "from": "project",
        "version": "00.00.00.00",
        "printer_model": "Bambu Lab X1 Carbon",
        "printer_settings_id": "Bambu Lab X1 Carbon 0.4 nozzle",
        "curr_bed_type": "Supertack Plate",
        "layer_height": "0.2",
        "initial_layer_print_height": "0.2",
        "nozzle_diameter": ["0.4"],
        "filament_type": ["PLA"] * 9,
        "nozzle_temperature": [str(200 + (i % 3) * 5) for i in range(17)],
        "nozzle_temperature_initial_layer": [str(205 + (i % 3) * 5) for i in range(17)],
        "supertack_plate_temp": nine(40, 60, 40, 40, 40, 40, 40, 40, 0),
        "hot_plate_temp": nine(50, 65, 50, 50, 50, 50, 50, 50, 30),
        "cool_plate_temp": nine(35, 35, 35, 35, 35, 35, 35, 35, 35),
    }

def with_colors(values):
    return {**values, "filament_colour": COLOR_LIST, "filament_type": ["PLA"] * 4}

PLATE_JSON = {
    1: {"first_layer_time": 123.4567, "bed_type": "supertack_plate"},
    2: {"first_layer_time": 88.04, "bed_type": "textured_plate"},
    # plate 3 deliberately has no plate_3.json
    4: {"first_layer_time": 250.96, "bed_type": "supertack_plate"},
}

def main(out, colors=False):
    entries = [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", RELS),
        ("3D/3dmodel.model", model_xml()),
        ("Metadata/model_settings.config", MODEL_SETTINGS_COLORS if colors else MODEL_SETTINGS),
        ("Metadata/slice_info.config", SLICE_INFO),
        ("Metadata/project_settings.config", json.dumps(with_colors(settings()) if colors else settings(), indent=4) + "\n"),
    ]
    for n, extra in PLATE_JSON.items():
        body = {"bbox_all": [0.0, 0.0, 10.0, 10.0], "bbox_objects": [], "bed_type": extra["bed_type"],
                "filament_colors": [], "filament_ids": [], "first_extruder": 0,
                "first_layer_time": extra["first_layer_time"], "is_seq_print": False,
                "nozzle_diameter": 0.4000000059604645, "version": 2}
        entries.append((f"Metadata/plate_{n}.json", json.dumps(body)))
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.comment = COMMENT
        for name, content in entries:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, content)

if __name__ == "__main__":
    args = sys.argv[1:]
    if args[:1] == ["--colors"]:
        main(args[1] if len(args) > 1 else "synthetic-bambu-colors.3mf", colors=True)
    else:
        main(args[0] if args else "synthetic-bambu-project.3mf")
