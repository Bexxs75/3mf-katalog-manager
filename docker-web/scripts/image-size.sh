#!/usr/bin/env bash
# Size of an image as a registry stores it: the sum of its gzip-compressed layers, which is what
# a pull downloads and what the 250 MB budget of the web image is measured in. Layers that are
# already gzip blobs count with their size; uncompressed ones (local builds) are compressed with
# gzip level 6, the level docker push and buildx use by default.
# Usage: docker-web/scripts/image-size.sh IMAGE   (nothing is pushed)
set -euo pipefail
image=${1:?usage: image-size.sh IMAGE}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
docker save -o "$work/image.tar" "$image"
python3 - "$work/image.tar" "$image" <<'EOF'
import json, sys, tarfile, zlib

archive, image = sys.argv[1], sys.argv[2]
with tarfile.open(archive) as tar:
    manifest = json.load(tar.extractfile("manifest.json"))[0]
    total_c = total_u = 0
    for i, layer in enumerate(manifest["Layers"], 1):
        f = tar.extractfile(layer)
        head = f.read(2)
        f.seek(0)
        if head == b"\x1f\x8b":
            compressed = tar.getmember(layer).size
            d = zlib.decompressobj(31)
            size = 0
            while chunk := f.read(1 << 20):
                size += len(d.decompress(chunk))
        else:
            c = zlib.compressobj(6, zlib.DEFLATED, 31)
            compressed = size = 0
            while chunk := f.read(1 << 20):
                size += len(chunk)
                compressed += len(c.compress(chunk))
            compressed += len(c.flush())
        total_c += compressed
        total_u += size
        print(f"layer {i:2d}: {compressed / 1e6:8.1f} MB compressed  {size / 1e6:8.1f} MB unpacked")
print(f"{image}: {len(manifest['Layers'])} layers, compressed {total_c / 1e6:.1f} MB ({total_c} bytes), unpacked {total_u / 1e6:.1f} MB")
EOF
