"""A stand-in Gizlet Store for the end-to-end test.

GET /v1/gizlets/github.com/<owner>/<repo>/install answers with
ROOT/<owner>__<repo>.json, or 404 as the Store does for a repository with no
release; /files/<name> serves ROOT/files/<name>. Usage: fixture_store.py ROOT PORT"""

import http.server
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
port = int(sys.argv[2])
PREFIX = "/v1/gizlets/github.com/"


class Handler(http.server.BaseHTTPRequestHandler):
    def answer(self, status: int, body: bytes, kind: str) -> None:
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        path = self.path.split("?", 1)[0]
        if path.startswith(PREFIX) and path.endswith("/install"):
            repo = path[len(PREFIX):-len("/install")]
            release = root / (repo.replace("/", "__") + ".json")
            if release.is_file():
                return self.answer(200, release.read_bytes(), "application/json")
            error = {"error": {"code": "not_found", "message": "no release"}}
            return self.answer(404, json.dumps(error).encode(), "application/json")
        if path.startswith("/files/"):
            file = root / "files" / Path(path).name
            if file.is_file():
                return self.answer(200, file.read_bytes(), "application/octet-stream")
        self.answer(404, b"{}", "application/json")

    def log_message(self, *args) -> None:
        pass


http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
