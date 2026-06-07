# Releases Site — Implementation Brief

A dead-simple **static** GitHub Pages site that lists this project's published
releases by fetching them live from the GitHub REST API. No build step, no
framework, no backend. This branch (`site`) is the home for the site.

> **For a fresh session:** everything you need is in this file. Build the files in
> the "Files to create" section on the `site` branch, then follow "Deployment".
> Do not touch the Rust app — the site is independent.

---

## Context

The release pipeline (on `main`/`dev`) publishes GitHub Releases with downloadable
assets: a macOS `.dmg`, a Windows `*-setup.exe` installer, a portable `*-x86_64.zip`,
and `SHA256SUMS.txt`. This site gives users a friendly page to find and download the
latest build instead of the raw GitHub Releases tab. A custom domain will be added
later via repo settings.

- **Repo slug:** `devnull03/islandora_workbench_gui`
- **API endpoint (no auth needed, public repo):**
  `GET https://api.github.com/repos/devnull03/islandora_workbench_gui/releases?per_page=20`
  - Returns published releases, newest first. Each has: `tag_name`, `name`,
    `published_at`, `html_url`, `body` (markdown notes), `prerelease`, and
    `assets[]` (each: `name`, `browser_download_url`, `size`, `download_count`).
  - **Draft releases are NOT returned** by this endpoint — exactly what we want
    (the release workflow creates drafts; they appear here only once published).

---

## Files to create (on the `site` branch)

```
index.html      # markup + <link> to styles.css + <script> app.js
styles.css      # minimal styling (system font stack, centered column, cards)
app.js          # fetch releases, render, cache; no dependencies required
.nojekyll       # empty file — disables Jekyll so nothing is rewritten
CNAME           # OPTIONAL placeholder; add only when the custom domain is chosen
README.md       # one-liner: what this branch is + how it deploys
```

### `index.html`
- A header: app name + a short tagline + a link to the GitHub repo.
- A `<main id="releases">` container that `app.js` fills in.
- A footer noting builds are unsigned (mirror the release-notes wording) and linking
  to the full Releases page (`.../releases`).
- Load `app.js` with `defer`.

### `app.js` (behaviour)
1. **Cache-first render:** read `localStorage["releases_cache"]` (a `{ ts, data }`
   object). If present and `Date.now() - ts < 10*60*1000`, render `data` immediately.
2. **Fetch fresh:** `fetch(API_URL, { headers: { Accept: "application/vnd.github+json" } })`,
   then cache `{ ts: Date.now(), data }` and re-render.
3. **Render each release as a card:**
   - Title: `name || tag_name`; badge **Latest** on the first non-prerelease,
     **Pre-release** when `prerelease` is true.
   - `published_at` formatted as a local date.
   - Notes: render `body` markdown. Either (a) pull `marked` from a CDN
     (`https://cdn.jsdelivr.net/npm/marked/marked.min.js`) and `marked.parse(body)`,
     or (b) to stay zero-dependency, escape `body` and show it in a `<pre>`. Pick one.
   - **Download list** from `assets[]`: one row per asset with a friendly label and a
     humanized size (`KB`/`MB`). Map by filename:
     `*-setup.exe` → "Windows installer", `*-x86_64.zip` → "Windows (portable)",
     `*-universal.dmg` → "macOS (Intel + Apple Silicon)", `SHA256SUMS.txt` → "Checksums".
4. **States:** loading placeholder; empty → "No releases yet" + link to Releases page;
   fetch error → friendly message + link to Releases page (and keep any cached render).

### `styles.css`
- System font stack, max-width ~760px centered column, light/dark via
  `@media (prefers-color-scheme: dark)`. Cards with a subtle border/radius. No CSS
  framework.

---

## Deployment

GitHub Pages, **deploy from a branch** (simplest for a static site):

1. Repo **Settings → Pages → Build and deployment → Source: "Deploy from a branch"**.
2. **Branch: `site`**, folder **`/ (root)`**. Save.
3. Wait for the Pages build; the URL is
   `https://devnull03.github.io/islandora_workbench_gui/`.
4. **Custom domain (later):** set it in Settings → Pages; that writes a `CNAME` file
   to this branch automatically (so the `CNAME` listed above is optional to pre-create).

> Alternative: serve from `/docs` on `main`. Not recommended here — keeping the site
> on its own `site` branch keeps the app history clean.

---

## Caveats

- **Rate limit:** unauthenticated API = 60 requests/hour/IP. The 10-minute
  `localStorage` cache keeps normal visitors well under it. Never commit a token.
- **CORS:** `api.github.com` sends permissive CORS headers, so browser `fetch` works
  directly from Pages — no proxy needed.
- **Drafts hidden:** a release shows up here only after it's published from the draft
  the release workflow creates.

---

## Acceptance checklist

- [ ] Opening the Pages URL lists releases newest-first, "Latest" on the top stable one.
- [ ] Each release shows its assets as labeled download links with human-readable sizes.
- [ ] Release notes render (markdown or escaped `<pre>`).
- [ ] Empty and error states render gracefully (no blank page, no uncaught errors).
- [ ] Reload within 10 min serves from cache (verify only one network call in devtools).
- [ ] Works in light and dark mode; readable on mobile width.
- [ ] No secrets/tokens in the committed code.

---

## Out of scope (for now)

Only this plan lives on the branch right now — no `index.html`/`app.js` yet. Implement
in a fresh session per the sections above.
