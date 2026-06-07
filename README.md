# Releases site (`site` branch)

A small [Astro](https://astro.build) site that lists this project's published
GitHub Releases with download links. It is **independent of the Rust app** — this
branch contains only the site.

## How it deploys

GitHub Pages via a **custom GitHub Actions workflow** (Settings → Pages → Source:
*GitHub Actions*):

- **`.github/workflows/deploy-site.yml`** (this branch) builds the site and
  deploys it. Release data is fetched **at build time** using the workflow's
  `GITHUB_TOKEN`, then pre-rendered to static HTML — nothing is fetched in the
  browser and no token ships to clients.
- Triggers: every push to `site`, plus manual *Run workflow* (`workflow_dispatch`).

### Auto-refresh when a release is published

The `release` event only fires for workflows on the **default branch**, so a tiny
companion workflow lives on `main`:

```yaml
# .github/workflows/redeploy-site-on-release.yml  (on main)
name: Redeploy site on release publish
on:
  release:
    types: [published]
  workflow_dispatch:
permissions:
  actions: write
jobs:
  trigger:
    runs-on: ubuntu-latest
    steps:
      - run: gh workflow run deploy-site.yml --ref site
        env:
          GH_TOKEN: ${{ github.token }}
```

Publishing a draft release ⇒ that workflow dispatches `deploy-site.yml` on `site`
⇒ the site rebuilds with the new release.

## Local development

```sh
npm install
npm run dev      # http://localhost:4321/islandora_workbench_gui/
npm run build    # outputs to dist/
npm run preview  # serve the built dist/ under the project base path
```

Local builds use the anonymous GitHub API (60 req/hr). The site URL is
`https://devnull03.github.io/islandora_workbench_gui/`.
