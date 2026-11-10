#!/usr/bin/env python3
"""Run the Synology window in a normal browser, without DSM.

Serves nas/ui at http://127.0.0.1:5080/webman/3rdparty/Turks/ and runs the
real `turks cgi` bridge for api.cgi, the way DSM does. Start the service
first with sign-in checks off:

    TURKS_NO_AUTH=1 TURKS_DATA_DIR=/tmp/turks-dev target/debug/turks daemon
    python3 nas/dev/serve.py target/debug/turks
"""
import os
import subprocess
import sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

HERE = os.path.dirname(os.path.abspath(__file__))
UI = os.path.join(HERE, "..", "ui")
BIN = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "target/debug/turks")
PORT = int(os.environ.get("PORT", "5080"))
PREFIX = "/webman/3rdparty/Turks/"


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=UI, **k)

    def log_message(self, *a):
        pass

    def do_GET(self):
        self.route("GET")

    def do_POST(self):
        self.route("POST")

    def route(self, method):
        url = urlparse(self.path)
        if url.path == "/webman/login.cgi":
            return self.send_bytes(200, b'{"SynoToken":"dev"}', "application/json")
        if url.path == PREFIX + "api.cgi":
            return self.cgi(method, url.query)
        if url.path in ("/", PREFIX[:-1]):
            self.send_response(302)
            self.send_header("Location", PREFIX + "index.html")
            self.end_headers()
            return
        if url.path.startswith(PREFIX) and method == "GET":
            self.path = "/" + url.path[len(PREFIX):]
            return super().do_GET()
        self.send_bytes(404, b"not found", "text/plain")

    def cgi(self, method, query):
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        env = dict(os.environ, REQUEST_METHOD=method, QUERY_STRING=query, CONTENT_LENGTH=str(len(body)),
                   HTTP_COOKIE="id=dev", HTTP_X_SYNO_TOKEN=self.headers.get("X-SYNO-TOKEN", ""), REMOTE_ADDR="127.0.0.1")
        out = subprocess.run([BIN, "cgi"], input=body, env=env, capture_output=True).stdout
        head, _, payload = out.partition(b"\r\n\r\n")
        status, headers = 200, []
        for line in head.decode("latin-1").split("\r\n"):
            k, _, v = line.partition(":")
            if k.lower() == "status":
                status = int(v.strip().split()[0])
            elif k:
                headers.append((k, v.strip()))
        self.send_response(status)
        for k, v in headers:
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def send_bytes(self, code, data, kind):
        self.send_response(code)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


if __name__ == "__main__":
    print(f"http://127.0.0.1:{PORT}{PREFIX}index.html", flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
