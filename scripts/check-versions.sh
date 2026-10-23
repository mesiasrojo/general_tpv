#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

tag="${1:-}"
expected="${tag#v}"

workspace_version="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "tpv-domain") | .version')"
tauri_version="$(jq -r '.version' apps/desktop/src-tauri/tauri.conf.json)"
desktop_version="$(jq -r '.version' apps/desktop/package.json)"
ui_version="$(jq -r '.version' ui/package.json)"
backoffice_version="$(jq -r '.version' backoffice/package.json)"

versions=("$workspace_version" "$tauri_version" "$desktop_version" "$ui_version" "$backoffice_version")
labels=("cargo workspace" "tauri.conf.json" "apps/desktop" "ui" "backoffice")

for index in "${!versions[@]}"; do
  if [[ "${versions[$index]}" != "$workspace_version" ]]; then
    echo "version mismatch: ${labels[$index]}=${versions[$index]} expected $workspace_version" >&2
    exit 1
  fi
done

if [[ -n "$expected" && "$workspace_version" != "$expected" ]]; then
  echo "tag version $expected does not match workspace version $workspace_version" >&2
  exit 1
fi

echo "versions consistent: $workspace_version"
