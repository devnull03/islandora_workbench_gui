#!/usr/bin/env bash
# Builds the update manifest the app polls: what the newest release is, where its notes are, and
# which artifact belongs to which platform.
#
# Usage: scripts/build-update-manifest.sh <version> <tag> <dist-dir> <repo-url> > stable.json
#
# The manifest is published as a release asset, which makes
# `<repo>/releases/latest/download/stable.json` a stable URL that always resolves to the newest
# non-prerelease release. That is the whole distribution mechanism for now.
#
# The shape is deliberately the one a *signed* feed would carry — `schema`, per-artifact `sha256`
# and `size` — even though the client only reads `version` and `release_notes_url` today. When
# updates become self-applying, the payload stops changing: it just gets wrapped in a signature
# envelope and served from somewhere the release workflow cannot forge.
set -euo pipefail

version="$1"
tag="$2"
dist="$3"
repo_url="$4"

# `kind` names the installer format, not just the file, because that is what an applying updater
# has to branch on: an NSIS installer is run with /S, a zip is unpacked in place, a dmg is mounted.
artifact_kind() {
  case "$1" in
    *-setup.exe) echo "windows-nsis" ;;
    *.zip)       echo "windows-portable" ;;
    *.dmg)       echo "macos-bundle" ;;
    *)           echo "unknown" ;;
  esac
}

artifact_arch() {
  case "$1" in
    *universal*) echo "universal" ;;
    *)           echo "x86_64" ;;
  esac
}

artifact_os() {
  case "$1" in
    *.dmg) echo "macos" ;;
    *)     echo "windows" ;;
  esac
}

entries=""
for path in "$dist"/*.dmg "$dist"/*.zip "$dist"/*-setup.exe; do
  [ -e "$path" ] || continue
  name="$(basename "$path")"
  entries="$entries$(cat <<EOF
    {
      "kind": "$(artifact_kind "$name")",
      "os": "$(artifact_os "$name")",
      "arch": "$(artifact_arch "$name")",
      "url": "$repo_url/releases/download/$tag/$name",
      "size": $(stat -c%s "$path"),
      "sha256": "$(sha256sum "$path" | cut -d' ' -f1)"
    },
EOF
)"
done

# Trim the trailing comma the loop leaves behind — JSON has no forgiving mode.
entries="${entries%,}"

cat <<EOF
{
  "schema": 1,
  "channel": "stable",
  "version": "$version",
  "published_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "release_notes_url": "$repo_url/releases/tag/$tag",
  "artifacts": [
$entries
  ]
}
EOF
