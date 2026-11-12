"""Local HTTP server that serves a Support Fins publish folder for update tests.

Serves VERSION.json and engine.iife.js (and any other files) from a folder given
on the command line. Listens on 127.0.0.1 at a port given on the command line
(0 = pick a free port). Prints the bound port when ready.

Usage:
  python dev/parity/fins_update_server.py <folder> <port>
"""
from __future__ import annotations

import http.server
import sys
from functools import partial
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: fins_update_server.py <folder> <port>", file=sys.stderr)
        return 2
    folder = Path(sys.argv[1]).resolve()
    if not folder.is_dir():
        print(f"fins_update_server: not a folder: {folder}", file=sys.stderr)
        return 1
    try:
        port = int(sys.argv[2])
    except ValueError:
        print(f"fins_update_server: bad port {sys.argv[2]!r}", file=sys.stderr)
        return 2

    handler = partial(http.server.SimpleHTTPRequestHandler, directory=str(folder))
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", port), handler)
    bound = httpd.server_address[1]
    print(bound, flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
