#!/usr/bin/env python3
"""Distribution usage stats (per docs/EXECPLAN_DIST_PERMALINK_AND_METRICS.md).

Reads three sources and prints a report:
  1. Permanent per-day history from Workers KV (written by the Worker's
     nightly rollup; survives Analytics Engine's ~90-day retention).
  2. Recent detail (last 7 days' version spread + country split) straight
     from Analytics Engine.
  3. All-time downloads per release from the GitHub Releases API.

Credentials come from the repo-root .env (gitignored):
  CF_ANALYTICS_TOKEN  - API token with Account Analytics: Read
                        + Workers KV Storage: Read
  CF_ACCOUNT_ID       - Cloudflare account id
  CF_KV_NAMESPACE_ID  - id of the HISTORY namespace (see infra/worker/wrangler.toml)

Usage (from the repo root):  python scripts/dist_stats.py [--days 30]

Each section fails soft: a network/auth error prints a warning and the
remaining sections still run. Stdlib only, no pip installs.
"""

import argparse
import json
import sys
import urllib.error
import urllib.request
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CF_API = "https://api.cloudflare.com/client/v4"
DIST_REPO = "tia-tools/releases"
DATASET = "dist_metrics"


def load_env():
    env = {}
    path = REPO_ROOT / ".env"
    if not path.exists():
        sys.exit(f"error: {path} not found (holds CF_ANALYTICS_TOKEN etc.)")
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, _, value = line.partition("=")
            env[key.strip()] = value.strip().strip('"')
    missing = [k for k in ("CF_ANALYTICS_TOKEN", "CF_ACCOUNT_ID", "CF_KV_NAMESPACE_ID") if k not in env]
    if missing:
        sys.exit(f"error: .env is missing {', '.join(missing)}")
    return env


def http(url, token=None, data=None, ua=None):
    headers = {}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if ua:
        headers["User-Agent"] = ua
    req = urllib.request.Request(url, data=data, headers=headers)
    with urllib.request.urlopen(req, timeout=30) as res:
        return res.read().decode("utf-8")


def section_history(env, days):
    print(f"== Daily history, last {days} days (KV rollup) ==")
    base = f"{CF_API}/accounts/{env['CF_ACCOUNT_ID']}/storage/kv/namespaces/{env['CF_KV_NAMESPACE_ID']}"
    keys = json.loads(http(f"{base}/keys?prefix=daily/&limit=1000", env["CF_ANALYTICS_TOKEN"]))
    names = sorted(k["name"] for k in keys.get("result", []))[-days:]
    if not names:
        print("  (no rollup keys yet - the nightly cron hasn't produced data)")
        return
    print(f"  {'date':<12}{'checks':>7}{'uniq':>6}{'dl(app)':>9}{'dl(web)':>9}  versions")
    for name in names:
        d = json.loads(http(f"{base}/values/{name}", env["CF_ANALYTICS_TOKEN"]))
        vers = ", ".join(f"{v}x{n}" for v, n in sorted(d.get("versions", {}).items(), reverse=True))
        print(
            f"  {d['date']:<12}{d['checks']:>7}{d['unique_users']:>6}"
            f"{d['downloads_app']:>9}{d['downloads_browser']:>9}  {vers}"
        )


def section_recent(env):
    print("== Recent detail, last 7 days (Analytics Engine, live) ==")
    since = (datetime.now(timezone.utc) - timedelta(days=7)).strftime("%Y-%m-%d %H:%M:%S")
    # Uniques are computed here from grouped (ver, country, bucket) rows,
    # same approach as the Worker's rollup; sum(_sample_interval) is the
    # sampling-correct row count.
    sql = (
        f"SELECT blob1 AS event, blob2 AS ver, blob3 AS country, blob4 AS bucket, "
        f"sum(_sample_interval) AS n FROM {DATASET} "
        f"WHERE timestamp >= toDateTime('{since}') "
        f"GROUP BY event, ver, country, bucket FORMAT JSON"
    )
    url = f"{CF_API}/accounts/{env['CF_ACCOUNT_ID']}/analytics_engine/sql"
    rows = json.loads(http(url, env["CF_ANALYTICS_TOKEN"], data=sql.encode("utf-8"))).get("data", [])
    checks = sum(float(r["n"]) for r in rows if r["event"] == "check")
    buckets = {r["bucket"] for r in rows if r["event"] == "check"}
    ver_buckets, country_buckets = defaultdict(set), defaultdict(set)
    for r in rows:
        if r["event"] != "check":
            continue
        if r["ver"]:
            ver_buckets[r["ver"]].add(r["bucket"])
        country_buckets[r["country"] or "??"].add(r["bucket"])
    print(f"  update checks: {checks:.0f}   unique users: {len(buckets)}")
    for ver, b in sorted(ver_buckets.items(), reverse=True):
        print(f"  version {ver:<10} {len(b)} user(s)")
    for cc, b in sorted(country_buckets.items(), key=lambda kv: -len(kv[1])):
        print(f"  country {cc:<10} {len(b)} user(s)")


def section_github():
    print(f"== All-time downloads per release (GitHub {DIST_REPO}) ==")
    releases = json.loads(
        http(f"https://api.github.com/repos/{DIST_REPO}/releases", ua="dist-stats-script")
    )
    for rel in releases:
        count = sum(a.get("download_count", 0) for a in rel.get("assets", []) if a["name"].endswith(".zip"))
        print(f"  {rel.get('tag_name', '?'):<12}{count:>6}")


def main():
    parser = argparse.ArgumentParser(description="Print distribution usage stats.")
    parser.add_argument("--days", type=int, default=30, help="days of KV history to show (default 30)")
    args = parser.parse_args()
    env = load_env()
    for section in (lambda: section_history(env, args.days), lambda: section_recent(env), section_github):
        try:
            section()
        except (urllib.error.URLError, urllib.error.HTTPError, json.JSONDecodeError, KeyError) as err:
            print(f"  warning: section failed: {err}")
        print()


if __name__ == "__main__":
    main()
