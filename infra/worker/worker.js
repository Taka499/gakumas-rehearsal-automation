// Distribution front for rehearsal-automation.tia.run (per docs/adr/0011).
// Per-app subdomain: each future tia-tools app gets its own single-level
// subdomain; the bare tia.run is intentionally unbound (reserved for a
// brand landing page someday).
//
// Stateless: everything is derived on the fly from the dist repo's
// GitHub Releases API, edge-cached for CACHE_TTL seconds, so publishing a
// release via /release is the only state change anywhere.
//
// Routes:
//   GET /latest.json       -> update manifest {version, notes, url, sha256}
//   GET /download          -> 302 to the latest release's zip (permanent share link)
//   GET /download/<asset>  -> 302 to that GitHub release asset
//   GET / or /latest       -> 302 to the dist repo's latest-release page

const REPO = "tia-tools/releases";
const RELEASES_PAGE = `https://github.com/${REPO}/releases`;
const API_LATEST = `https://api.github.com/repos/${REPO}/releases/latest`;
const CACHE_TTL = 300; // seconds; keeps us far under GitHub's 60 req/hr/IP anon limit
const DOMAIN = "rehearsal-automation.tia.run";

export default {
  async fetch(request, env, ctx) {
    const path = new URL(request.url).pathname;
    try {
      if (path === "/latest.json") {
        ctx.waitUntil(recordEvent(env, request, "check", ""));
        return await latestJson(env);
      }
      if (path === "/download" || path === "/download/")
        return await download(env, request, ctx, null);
      if (path.startsWith("/download/"))
        return await download(env, request, ctx, decodeURIComponent(path.slice("/download/".length)));
      if (path === "/" || path === "/latest")
        return Response.redirect(`${RELEASES_PAGE}/latest`, 302);
      return new Response("Not found\n", { status: 404 });
    } catch (err) {
      return new Response("Upstream error\n", { status: 502 });
    }
  },

  // Nightly (cron in wrangler.toml): preserve per-day aggregates in KV
  // before Analytics Engine's ~90-day retention discards the raw events.
  async scheduled(event, env, ctx) {
    ctx.waitUntil(rollup(env));
  },
};

// ---- Anonymous usage metrics (per docs/adr/0012) ----------------------
//
// Event row schema (positions are load-bearing: the rollup cron and
// scripts/dist_stats.py address these as blob1..blob5 in SQL):
//   blobs[0] event type: "check" | "download"
//   blobs[1] client app version parsed from User-Agent, "" for browsers
//   blobs[2] country code from request.cf.country, "" if absent
//   blobs[3] daily anonymous bucket (see dailyBucket)
//   blobs[4] downloaded asset name, "" for checks
// No raw IP and no persistent identifier is ever written; the bucket
// rotates because the UTC date is part of the hash input.

async function recordEvent(env, request, type, extra) {
  try {
    if (!env.METRICS) return; // binding absent in local dev — never break the user path
    const ip = request.headers.get("CF-Connecting-IP") || "";
    const day = new Date().toISOString().slice(0, 10);
    const bucket = await dailyBucket(ip, day, env.HASH_SALT || "");
    env.METRICS.writeDataPoint({
      blobs: [
        type,
        parseAppVersion(request.headers.get("User-Agent") || ""),
        (request.cf && request.cf.country) || "",
        bucket,
        extra || "",
      ],
      doubles: [1],
      indexes: [bucket],
    });
  } catch (err) {
    // Metrics are best-effort by design; swallow everything.
  }
}

// Truncated SHA-256(ip|utc-date|salt). Irreversible without the salt
// (a wrangler secret), and a different value every UTC day, so buckets
// cannot be chained into a cross-day profile.
async function dailyBucket(ip, day, salt) {
  const data = new TextEncoder().encode(`${ip}|${day}|${salt}`);
  const digest = await crypto.subtle.digest("SHA-256", data);
  return [...new Uint8Array(digest)]
    .slice(0, 8)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function parseAppVersion(userAgent) {
  const m = /^gakumas-rehearsal-automation\/(\d+\.\d+\.\d+)/.exec(userAgent);
  return m ? m[1] : "";
}

// ---- Nightly rollup: Analytics Engine -> KV ---------------------------
//
// Writes one JSON value per UTC day under KV key "daily/<YYYY-MM-DD>":
//   {date, checks, unique_users, downloads_app, downloads_browser,
//    versions: {ver: unique users}, countries: {cc: unique users}}
// Yesterday is always (re)written — idempotent by key. The 6 days before
// it are backfilled only if missing, so cron outages up to a week
// self-heal while the raw events still exist in AE.

async function rollup(env) {
  for (let back = 1; back <= 7; back++) {
    const day = new Date(Date.now() - back * 86400e3).toISOString().slice(0, 10);
    const key = `daily/${day}`;
    if (back > 1 && (await env.HISTORY.get(key)) !== null) continue;
    const agg = await aggregateDay(env, day);
    if (agg) await env.HISTORY.put(key, JSON.stringify(agg));
  }
}

async function aggregateDay(env, day) {
  // One grouped query per day; uniques are counted in JS from the
  // (event, version, country, bucket) combinations, sidestepping any
  // question of count(DISTINCT ...) support in the AE SQL dialect.
  // sum(_sample_interval), not count(): correct under AE sampling.
  const next = new Date(new Date(`${day}T00:00:00Z`).getTime() + 86400e3)
    .toISOString()
    .slice(0, 10);
  const sql =
    `SELECT blob1 AS event, blob2 AS ver, blob3 AS country, blob4 AS bucket, ` +
    `sum(_sample_interval) AS n FROM dist_metrics ` +
    `WHERE timestamp >= toDateTime('${day} 00:00:00') ` +
    `AND timestamp < toDateTime('${next} 00:00:00') ` +
    `GROUP BY event, ver, country, bucket FORMAT JSON`;
  const res = await fetch(
    `https://api.cloudflare.com/client/v4/accounts/${env.ACCOUNT_ID}/analytics_engine/sql`,
    {
      method: "POST",
      headers: { Authorization: `Bearer ${env.CF_ANALYTICS_TOKEN}` },
      body: sql,
    }
  );
  if (!res.ok) return null; // token/API trouble: skip, next cron retries via backfill
  const rows = (await res.json()).data || [];

  let checks = 0;
  let downloadsApp = 0;
  let downloadsBrowser = 0;
  const checkBuckets = new Set();
  const verBuckets = {};
  const countryBuckets = {};
  for (const r of rows) {
    const n = Number(r.n) || 0;
    if (r.event === "check") {
      checks += n;
      checkBuckets.add(r.bucket);
      if (r.ver) (verBuckets[r.ver] ||= new Set()).add(r.bucket);
      (countryBuckets[r.country || "??"] ||= new Set()).add(r.bucket);
    } else if (r.event === "download") {
      if (r.ver) downloadsApp += n;
      else downloadsBrowser += n;
    }
  }
  const sizes = (m) =>
    Object.fromEntries(Object.entries(m).map(([k, s]) => [k, s.size]));
  return {
    date: day,
    checks,
    unique_users: checkBuckets.size,
    downloads_app: downloadsApp,
    downloads_browser: downloadsBrowser,
    versions: sizes(verBuckets),
    countries: sizes(countryBuckets),
  };
}

function ghHeaders(env) {
  // GitHub rejects requests without a User-Agent.
  const headers = {
    "User-Agent": `${DOMAIN}-dist-worker`,
    Accept: "application/vnd.github+json",
  };
  // Optional: `wrangler secret put GITHUB_TOKEN` (the BOT's PAT, never a
  // personal one) if anonymous rate limits ever become a problem.
  if (env.GITHUB_TOKEN) headers.Authorization = `Bearer ${env.GITHUB_TOKEN}`;
  return headers;
}

async function getLatestRelease(env) {
  const res = await fetch(API_LATEST, {
    headers: ghHeaders(env),
    cf: { cacheTtl: CACHE_TTL, cacheEverything: true },
  });
  if (!res.ok) return null;
  return res.json();
}

async function latestJson(env) {
  const rel = await getLatestRelease(env);
  if (!rel) return new Response("No release\n", { status: 502 });

  const assets = rel.assets || [];
  const zip = assets.find((a) => a.name.endsWith(".zip"));
  if (!zip) return new Response("No zip asset\n", { status: 404 });

  // sha256: prefer GitHub's own asset digest, fall back to the .sha256
  // sidecar file that /release uploads next to the zip.
  let sha256 = "";
  if (zip.digest && zip.digest.startsWith("sha256:")) {
    sha256 = zip.digest.slice("sha256:".length);
  } else {
    const sidecar = assets.find((a) => a.name === `${zip.name}.sha256`);
    if (sidecar) {
      const res = await fetch(sidecar.browser_download_url, {
        headers: { "User-Agent": `${DOMAIN}-dist-worker` },
        cf: { cacheTtl: CACHE_TTL, cacheEverything: true },
      });
      if (res.ok) sha256 = (await res.text()).trim().split(/\s+/)[0];
    }
  }

  const manifest = {
    version: (rel.tag_name || "").replace(/^v/, ""),
    notes: (rel.body || "").split(/\r?\n\r?\n/)[0].trim(),
    url: `https://${DOMAIN}/download/${encodeURIComponent(zip.name)}`,
    sha256: sha256.toLowerCase(),
  };
  return new Response(JSON.stringify(manifest, null, 2) + "\n", {
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": `public, max-age=${CACHE_TTL}`,
    },
  });
}

async function download(env, request, ctx, name) {
  const rel = await getLatestRelease(env);
  if (!rel) return new Response("No release\n", { status: 502 });
  const assets = rel.assets || [];
  // No name = the permanent link: whatever zip the latest release carries
  // (same selection rule as latestJson).
  const asset = name
    ? assets.find((a) => a.name === name)
    : assets.find((a) => a.name.endsWith(".zip"));
  if (!asset) return new Response("No such asset\n", { status: 404 });
  // Recorded only for resolved assets, so 404 probes don't count as downloads.
  ctx.waitUntil(recordEvent(env, request, "download", asset.name));
  return Response.redirect(asset.browser_download_url, 302);
}
