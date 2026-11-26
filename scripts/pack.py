"""Writes manifest.json from gizlet.toml into the app folder, then packs it
as <id>-<version>-<target>-<tier>.gzl with CHECKSUMS. Usage: pack.py APP DIST"""

import hashlib
import json
from pathlib import Path
import shutil
import sys
import tomllib
import zipfile

app, dist = (Path(arg) for arg in sys.argv[1:])
spec = tomllib.loads(Path("gizlet.toml").read_text(encoding="utf-8"))
package, = spec["packages"]
manifest = {"schema": 1}
for key in ("id", "name", "names", "version", "description", "descriptions", "color"):
    if key in spec:
        manifest[key] = spec[key]
manifest["runtime"] = spec.get("runtime", 1)
for key in ("target", "tier", "entry"):
    manifest[key] = package[key]
icon = Path(spec["icon"])
manifest["icon"] = f"icon{icon.suffix}"
shutil.copyfile(icon, app / manifest["icon"])
(app / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

dist.mkdir(parents=True, exist_ok=True)
output = dist / "{id}-{version}-{target}-{tier}.gzl".format(**manifest)
checksums = []
with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED, allowZip64=False) as archive:
    for path in sorted(app.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(app).as_posix()
        with path.open("rb") as source:
            checksums.append(f"{hashlib.file_digest(source, 'sha256').hexdigest()}  {relative}\n")
        archive.write(path, relative)
    archive.writestr("CHECKSUMS", "".join(checksums))
print(output)
