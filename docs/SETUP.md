# Setup & Release Runbook

How to stand this project up from scratch and how its CI/CD works. This is the
"do these things first" companion to `README.md` — especially before cutting a
release, since a release depends on several pieces being configured ahead of time.

---

## 1. What's in the repo

| Branch | Contents | Purpose |
|---|---|---|
| `main` | The Rust **GPUI** desktop app (`crates/*`) + release pipeline | Default branch; source of truth for the app and for tag-driven releases. |
| `dev` | Same app, integration branch | Where day-to-day work lands and CI runs before promoting to `main`. |
| `site` | An **Astro** static site (no Rust) | The public releases page deployed to GitHub Pages. Independent of the app. |

Workspace crates (versions are inherited from `[workspace.package].version`):
`crates/app` (binary `app`), `crates/settings`, `crates/workbench-integration`,
`crates/window-wrapper`.

---

## 2. Local prerequisites

**App (on `main`/`dev`):**
- Rust **stable** toolchain — `rust-toolchain.toml` pins the channel and pulls in
  `rustfmt` + `clippy`. Run `rustup update stable` periodically to match CI.
- The app only targets **Windows + macOS** (gpui needs heavy system libs on Linux),
  so build/test there.
- **Runtime dependency:** the app drives the external
  [Islandora Workbench](https://github.com/mjordan/islandora_workbench) Python tool.
  End users need `python` + `uv` and that tool installed separately — it is **not**
  bundled in the release artifacts.

**Site (on `site`):**
- Node 20+ and npm. `npm install`, then `npm run dev`
  (`http://localhost:4321/islandora_workbench_gui/`).

---

## 3. One-time GitHub setup (do this before the first release)

These are the "things to set up beforehand" — without them the pipelines fail or
produce nothing visible.

1. **GitHub Pages → GitHub Actions source.**
   Settings → Pages → Build and deployment → Source = **GitHub Actions**
   (API `build_type: "workflow"`). Required for `deploy-site.yml` to publish.
   *Already enabled for this repo.*

2. **Actions permissions.** Settings → Actions → General → Workflow permissions:
   allow workflows to write (the release job needs `contents: write`; the site
   dispatcher needs `actions: write`). These are also declared per-workflow.

3. **Branch protection (recommended).** Protect `main`: require the `CI` checks
   (Windows + macOS) to pass before merge. Note the release **tag** push is not a
   PR, so it bypasses protection by design.

4. **Code signing (optional, currently OFF).** Releases are **unsigned**:
   - Windows: SmartScreen "unknown publisher" warning.
   - macOS: Gatekeeper quarantine (`xattr -dr com.apple.quarantine ...`).
   To sign later you'd add secrets (Apple Developer ID cert + notarization creds,
   a Windows code-signing cert) and signing steps in `release.yml`. None exist yet,
   so no secrets are required to build today.

5. **Custom domain (optional).** Settings → Pages writes a `CNAME`; then set
   `site:` in `astro.config.mjs` to the domain and drop/empty `base`.

---

## 4. CI/CD pipelines

Four workflows, two on `main`, one on `site`, and CI on `dev`/PRs.

### `ci.yml` — quality gate (on `main`)
- **Triggers:** push to `dev`; PRs targeting `dev` or `main`.
- **Does:** on Windows + macOS, runs `cargo fmt --check`, `cargo clippy … -D warnings`,
  `cargo test`, `cargo build`. Cancels superseded runs to save minutes.
- **Why it matters:** a PR into `main` (including a release-prep PR) must be green.

### `release.yml` — build & publish artifacts (on `main`)
- **Trigger:** pushing a tag matching `v*`. Merging to `main` alone does nothing.
- **Guard:** the `version` job asserts the tag (minus `v`) **exactly equals**
  `Cargo.toml`'s `[workspace.package].version`. Mismatch ⇒ the build fails fast.
- **Builds:**
  - macOS: universal (x86_64 + aarch64) binary → `.app` → `.dmg`
    (`scripts/bundle-mac.sh`).
  - Windows: `.exe` → portable `*-x86_64.zip` + NSIS `*-setup.exe`
    (`scripts/installer.nsi`).
- **Publishes:** a **DRAFT** release with the artifacts + `SHA256SUMS.txt`. It does
  **not** set the pre-release flag — you choose that when you publish (§5).

### `deploy-site.yml` — build & deploy the site (on `site`)
- **Triggers:** push to `site`; `workflow_dispatch`.
- **Does:** `withastro/action` builds the Astro site, which **fetches the release
  list at build time** using the job's `GITHUB_TOKEN` (1000 req/hr, no token in the
  shipped HTML), then `actions/deploy-pages` publishes it.

### `redeploy-site-on-release.yml` — refresh the site on release (on `main`)
- **Trigger:** `release: published` (and `workflow_dispatch`).
- **Why it lives on `main`:** `release` events only fire for workflows on the
  **default branch**. The site's build is on `site`, so this small workflow bridges
  the gap: it runs `gh workflow run deploy-site.yml --ref site`.
- **Net effect:** publishing a release ⇒ the site rebuilds and shows it.

```
tag vX.Y.Z ─▶ release.yml (build dmg/zip/exe) ─▶ DRAFT release
                                                     │ (you publish, optionally pre-release)
                                                     ▼
                                          release: published
                                                     │
                              redeploy-site-on-release.yml (on main)
                                                     │ gh workflow run --ref site
                                                     ▼
                                    deploy-site.yml (on site) ─▶ Pages updates
```

---

## 5. Cutting a release (runbook)

1. **Land the code** on `main` (via PR; CI green).
2. **Bump the version** in `Cargo.toml` `[workspace.package].version` (inherited by
   all crates), and sync the lockfile (`cargo metadata` or any cargo build updates
   the member versions in `Cargo.lock`). Commit to `main`.
3. **Tag and push** — the tag must match the version exactly:
   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```
4. **Wait for `release.yml`** to finish; it leaves a **draft** release with the
   `.dmg`, `.zip`, `-setup.exe`, and `SHA256SUMS.txt`.
5. **Publish the draft** (Releases → edit the draft → *Publish release*). This is
   when the release becomes visible to the API and to the site.
6. Publishing fires `redeploy-site-on-release.yml` → the site rebuilds with the new
   release.

### Pre-releases (alpha / rc / beta)
- Use a **semver pre-release version**, e.g. `0.1.0-alpha.1`, in `Cargo.toml`, and
  tag `v0.1.0-alpha.1` (the version guard requires the match — `0.1.0-alpha.1`
  is a valid Cargo version).
- When publishing the draft, **check "Set as a pre-release"** (API `prerelease: true`).
  The site renders it with a **"Pre-release"** badge and never awards it the
  **"Latest"** badge — "Latest" only goes to the newest *stable* (non-pre-release)
  release.
- Drafts are hidden from the public API, so an in-progress release never leaks to
  the site until you publish it.

### Rolling back a bad tag
```sh
git push origin :refs/tags/v0.1.0   # delete remote tag
git tag -d v0.1.0                    # delete local tag
```
Then delete the draft/release in the GitHub UI if one was created.
