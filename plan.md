# Plan: Releases Site (GitHub Pages)

A dead-simple static site that lists the project's releases by fetching them live
from the GitHub REST API. No build step, no framework, no backend. A custom domain
will be added later in repo settings.

## Goal

Give users a friendly page to find and download the latest builds (the `.dmg`,
Windows installer/zip, and checksums attached to each GitHub Release), without
sending them to the raw GitHub Releases tab.

## Source of data

Public GitHub REST API (no auth token needed for a public repo):

```
GET https://api.github.com/repos/devnull03/islandora_workbench_gui/releases
```

- Returns published releases, newest first, each with `tag_name`, `name`,
  `published_at`, `body` (markdown notes), and an `assets[]` array
  (`name`, `browser_download_url`, `size`).
- **Draft** releases are NOT returned by this endpoint — which is exactly what we
  want (drafts stay private until published).

## Structure (single page, vanilla JS)

```
index.html      # markup + a little CSS (or a tiny styles.css)
app.js          # fetch releases, render cards, cache in localStorage
CNAME           # placeholder for the future custom domain (added when ready)
```

Rendering plan (`app.js`):
1. On load, read cached JSON from `localStorage` (if < ~10 min old) and render it
   immediately; then fetch fresh data and re-render.
2. For each release render a card: version/tag, published date, rendered notes,
   and a download link per asset (label by filename + human-readable size).
3. Mark the first (newest) release as **Latest**.
4. Link `SHA256SUMS.txt` prominently for verification.
5. Empty/error states: "No releases yet" and a graceful API-error message with a
   link to the GitHub Releases page.

Markdown notes: render with a tiny client-side markdown lib (e.g. `marked` via CDN)
or, to keep zero dependencies, show the notes as preformatted text. Decide at
implementation time.

## Deployment

- GitHub Pages serving this branch's root (Settings → Pages → Source: `site` branch),
  **or** move the site into `/docs` on `main` — decide at implementation time.
- Add a `CNAME` file once the custom domain is chosen.

## Caveats

- **Rate limit:** unauthenticated API is 60 requests/hour/IP. The `localStorage`
  cache keeps normal visitors well under this; no token is committed.
- **Drafts hidden:** the `/releases` endpoint omits drafts, so a release only
  appears here after it is published from the draft created by the release workflow.
- **CORS:** `api.github.com` sends permissive CORS headers, so the browser `fetch`
  works directly from GitHub Pages with no proxy.

## Out of scope (for now)

Per the request, this branch contains **only this plan** — no `index.html`/`app.js`
yet. Implementation follows once the plan is approved.
