"""Generate synthetic statistics only; never read the user's database."""
import json
import time
from pathlib import Path
now = int(time.time() * 1000)
def row(key, timestamp):
    return dict(platform="claude", source="local", dedup_key=key, session_id="acceptance-session", ts=timestamp, model=None, input_tokens=10, output_tokens=2, cache_read_tokens=0, cache_write_tokens=0, total_tokens=12)
output = Path(__file__).resolve().parents[2] / "output/acceptance"
output.mkdir(parents=True, exist_ok=True)
(output / "import.json").write_text(json.dumps(dict(schema_version=1, exported_at_ms=now, platform=None, requests=[row("old", now - 100 * 86400000), row("new", now)], sessions=[dict(platform="claude", session_id="acceptance-session", title="隔离验收会话")])), encoding="utf-8")
