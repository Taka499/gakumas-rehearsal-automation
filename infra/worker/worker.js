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
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    try {
      if (path === "/latest.json") return await latestJson(env);
      if (path === "/download" || path === "/download/")
        return await download(env, null);
      if (path.startsWith("/download/"))
        return await download(env, decodeURIComponent(path.slice("/download/".length)));
      if (path === "/" || path === "/latest")
        return Response.redirect(`${RELEASES_PAGE}/latest`, 302);
      return new Response("Not found\n", { status: 404 });
    } catch (err) {
      return new Response("Upstream error\n", { status: 502 });
    }
  },
};

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

async function download(env, name) {
  const rel = await getLatestRelease(env);
  if (!rel) return new Response("No release\n", { status: 502 });
  const assets = rel.assets || [];
  // No name = the permanent link: whatever zip the latest release carries
  // (same selection rule as latestJson).
  const asset = name
    ? assets.find((a) => a.name === name)
    : assets.find((a) => a.name.endsWith(".zip"));
  if (!asset) return new Response("No such asset\n", { status: 404 });
  return Response.redirect(asset.browser_download_url, 302);
}
