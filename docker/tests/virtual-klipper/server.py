#!/usr/bin/env python3
"""Virtual Klipper/Moonraker printer for the web printer-link test (Forgejo #42, C6).

Answers the Moonraker REST calls the catalog uses (/server/info, /server/history/list, thumbnails)
with the JSON shape of a real Moonraker (v0.8.x). A virtual print runs through the states
printing -> completed/cancelled: it shows up as "in_progress" in the history while it runs and
becomes a finished job afterwards, as Klipper's print_stats would record it. Test hooks (POST
/_sim/print, /_sim/reset) exist only for the test. Optional API key: PRINTER_API_KEY.
Standard library only, listens on all interfaces of the test network, never publishes ports.
"""
import json, os, threading, time, base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

KEY = os.environ.get("PRINTER_API_KEY", "")
MODE = os.environ.get("PRINTER_MODE", "")  # "redirect": answers every request with a 302 to the cloud metadata address
LOCK = threading.Lock()
JOBS = []  # newest last
COUNTER = [0x3F]
# 1x1 PNG
PNG = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")

def info():
    return {"result": {"klippy_connected": True, "klippy_state": "ready",
        "components": ["klippy_connection", "application", "database", "file_manager", "history", "job_state"],
        "failed_components": [], "registered_directories": ["config", "logs", "gcodes"], "warnings": [],
        "websocket_count": 0, "moonraker_version": "v0.8.0-209-gvirtual", "missing_klippy_requirements": [],
        "api_version": [1, 4, 0], "api_version_string": "1.4.0"}}

def finish(job, status, seconds):
    with LOCK:
        job["status"] = status
        job["end_time"] = time.time()
        job["print_duration"] = float(seconds)
        job["total_duration"] = float(seconds)
        if status != "completed":
            job["filament_used"] = job["filament_used"] / 2.0

def start_print(body):
    seconds = float(body.get("seconds", 1.0))
    name = body.get("filename", "Virtuell_PLA_0.2.gcode")
    mm = float(body.get("filament_mm", 3000.0))
    status = body.get("status", "completed")
    stem = name.rsplit(".", 1)[0]
    with LOCK:
        COUNTER[0] += 1
        job = {"job_id": "%06X" % COUNTER[0], "filename": name, "status": "in_progress",
               "start_time": time.time(), "end_time": None, "print_duration": 0.0, "total_duration": 0.0,
               "filament_used": mm, "exists": True,
               "metadata": {"slicer": "OrcaSlicer", "filament_type": body.get("material", "PLA"),
                            "filament_total": mm, "filament_weight_total": round(mm * 0.00241, 2),
                            "thumbnails": [{"width": 32, "height": 32, "size": len(PNG),
                                            "relative_path": ".thumbs/%s-32x32.png" % stem}]}}
        JOBS.append(job)
    threading.Timer(seconds, finish, (job, status, seconds)).start()
    return job["job_id"]

def history(query):
    q = {k: v[0] for k, v in parse_qs(query).items()}
    with LOCK:
        rows = list(JOBS)
    if "since" in q:
        rows = [j for j in rows if j["start_time"] >= float(q["since"])]
    if "before" in q:
        rows = [j for j in rows if j["start_time"] <= float(q["before"])]
    if q.get("order", "desc") == "desc":
        rows.reverse()
    start, limit = int(q.get("start", 0)), int(q.get("limit", 50))
    page = [dict(j) for j in rows[start:start + limit]]
    for j in page:
        if j["end_time"] is None:
            del j["end_time"]
    return {"result": {"count": len(rows), "jobs": page}}

class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, fmt, *args):
        # request line only, never headers (the API key travels there)
        print("vk %s" % (fmt % args), flush=True)
    def send(self, code, body, ctype="application/json"):
        data = body if isinstance(body, bytes) else json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def authorized(self):
        return not KEY or self.headers.get("X-Api-Key") == KEY
    def do_GET(self):
        u = urlparse(self.path)
        if MODE == "redirect":
            self.send_response(302)
            self.send_header("Location", "http://169.254.169.254/latest/meta-data/")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if u.path.startswith("/_sim/"):
            return self.sim(u)
        if not self.authorized():
            return self.send(401, {"error": {"code": 401, "message": "Unauthorized"}})
        if u.path == "/server/info":
            return self.send(200, info())
        if u.path == "/server/history/list":
            return self.send(200, history(u.query))
        if u.path.startswith("/server/files/gcodes/.thumbs/") or "/.thumbs/" in u.path:
            return self.send(200, PNG, "image/png")
        self.send(404, {"error": {"code": 404, "message": "Not Found"}})
    def do_POST(self):
        u = urlparse(self.path)
        n = int(self.headers.get("Content-Length", 0) or 0)
        raw = self.rfile.read(n) if n else b"{}"
        if not u.path.startswith("/_sim/"):
            return self.send(404, {"error": {"code": 404, "message": "Not Found"}})
        if u.path == "/_sim/print":
            return self.send(200, {"job_id": start_print(json.loads(raw or b"{}"))})
        if u.path == "/_sim/reset":
            with LOCK:
                JOBS.clear()
            return self.send(200, {"ok": True})
        self.send(404, {"error": "unknown"})
    def sim(self, u):
        self.send(200, {"ok": True})

if __name__ == "__main__":
    ThreadingHTTPServer(("0.0.0.0", 7125), Handler).serve_forever()
