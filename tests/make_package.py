"""Packs a folder as a test .gzl with manifest.json and CHECKSUMS, and writes
the Store's install answer for it. Usage:
make_package.py ID VERSION FOLDER FILES_DIR BASE_URL ANSWER_JSON [--corrupt]"""

import hashlib
import json
import sys
import zipfile
from pathlib import Path

app_id, version, folder, files, base_url, answer = sys.argv[1:7]
corrupt = "--corrupt" in sys.argv[7:]
folder, files = Path(folder), Path(files)
target, tier = "aarch64-unknown-linux-gnu", "lite"
manifest = {"schema": 1, "id": app_id, "name": f"Test {app_id.rsplit('.', 1)[-1]}", "version": version,
            "runtime": 1, "target": target, "tier": tier, "entry": "bin/launch"}
(folder / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
name = f"{app_id}-{version}-{target}-{tier}.gzl"
files.mkdir(parents=True, exist_ok=True)
checksums = []
with zipfile.ZipFile(files / name, "w", zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(p for p in folder.rglob("*") if p.is_file()):
        relative = path.relative_to(folder).as_posix()
        checksums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {relative}\n")
        archive.write(path, relative)
    archive.writestr("CHECKSUMS", "".join(checksums))
data = (files / name).read_bytes()
sha256 = hashlib.sha256(data + (b"x" if corrupt else b"")).hexdigest()
body = {"repo": "github.com/test", "id": app_id, "version": version, "tag": f"v{version}",
        "package": {"file": name, "target": target, "tier": tier, "entry": "bin/launch", "size": len(data),
                    "sha256": sha256, "url": f"{base_url}/files/{name}?sha256={sha256}"}}
Path(answer).write_text(json.dumps(body))
