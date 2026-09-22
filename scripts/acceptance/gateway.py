"""本地 HTTP 契约夹具，不读取用户密钥、不调用外网。"""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        key = self.headers.get("Authorization", "").removeprefix("Bearer ")
        status = {"test-unauthorized": 401, "test-forbidden": 403, "test-limited": 429, "test-server-error": 500}.get(key, 200)
        data = {"balance": 0 if key == "test-zero" else 12.5, "unit": "USD", "isValid": key != "test-invalid", "planName": "Acceptance fixture"}
        if key == "test-quota":
            data["subscription"] = {"daily_usage_usd": 2, "daily_limit_usd": 10, "weekly_usage_usd": 8, "weekly_limit_usd": 20}
        if key == "test-missing": data = {}
        body = b"not-json" if key == "test-malformed" else json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        if status == 429: self.send_header("Retry-After", "2")
        self.end_headers()
        self.wfile.write(body)

ThreadingHTTPServer(("127.0.0.1", 18765), Handler).serve_forever()
