#!/usr/bin/env python3
"""Mock Moonraker for trying the printer connection by hand (standard library only).

The app only talks to addresses in the home network, so bind to the LAN address of this
machine (--host), not to 127.0.0.1. Serves the fixtures of the test suite and can misbehave on
purpose: --flaky, --slow, --restart-every, --drip. It never talks to a real printer.

  python3 tools/mock_moonraker.py --host 192.168.1.20 --port 7125 --flaky 2
"""
import argparse
import json
import pathlib
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

FIXTURES = pathlib.Path(__file__).resolve().parent.parent / "crates/katalog-core/tests/fixtures/moonraker"


class State:
    def __init__(self, args):
        self.args = args
        self.count = 0
        self.lock = threading.Lock()
        self.info = (FIXTURES / "server_info_v0_8_0_209.json").read_bytes()
        self.history = pathlib.Path(args.history).read_bytes() if args.history else (FIXTURES / "history_completed.json").read_bytes()

    def next_number(self):
        with self.lock:
            self.count += 1
            return self.count


def make_handler(state):
    args = state.args

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, fmt, *rest):
            print(f"{self.address_string()} {fmt % rest}", flush=True)

        def reply(self, status, body):
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            self.end_headers()
            if args.drip:
                for i in range(len(body)):
                    self.wfile.write(body[i:i + 1])
                    self.wfile.flush()
                    time.sleep(args.drip)
            else:
                self.wfile.write(body)

        def do_GET(self):
            number = state.next_number()
            path = self.path.split("?")[0]
            if args.slow:
                time.sleep(args.slow)
            # Every Nth request: the connection ends without an answer, like a Moonraker that restarts.
            if args.restart_every and number % args.restart_every == 0:
                self.close_connection = True
                return
            # The first N requests to /server/info: 503, like a web server in front of a restarting Moonraker.
            if path == "/server/info" and number <= args.flaky:
                self.reply(503, b'{"error": "Service Unavailable"}')
                return
            if path == "/server/info":
                self.reply(200, state.info)
            elif path == "/server/history/list":
                self.reply(200, state.history)
            else:
                self.reply(404, b"{}")

    return Handler


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--host", required=True, help="LAN address to bind to (not 127.0.0.1: the app refuses it)")
    p.add_argument("--port", type=int, default=7125)
    p.add_argument("--flaky", type=int, default=0, metavar="N", help="answer the first N requests of /server/info with 503")
    p.add_argument("--slow", type=float, default=0.0, metavar="SECONDS", help="wait before every answer")
    p.add_argument("--restart-every", type=int, default=0, metavar="N", help="close every Nth request without an answer")
    p.add_argument("--drip", type=float, default=0.0, metavar="SECONDS", help="send the body one byte at a time")
    p.add_argument("--history", help="file with a /server/history/list answer instead of the fixture")
    args = p.parse_args()
    if args.history:
        json.loads(pathlib.Path(args.history).read_text())  # fail early on a broken file
    server = ThreadingHTTPServer((args.host, args.port), make_handler(State(args)))
    print(f"Mock Moonraker on http://{args.host}:{args.port} (Ctrl+C to stop)", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
