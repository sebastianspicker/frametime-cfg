#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
failed=0

fail() {
  printf '%s\n' "$1" >&2
  failed=1
}

manifest_has_path() {
  local manifest="$1" dependency="$2"
  grep -Eq "^${dependency}[[:space:]]*=[[:space:]]*\{[[:space:]]*path[[:space:]]*=" "$manifest"
}

domain="$root/crates/frametime-domain/Cargo.toml"
windows="$root/crates/frametime-windows/Cargo.toml"
app="$root/crates/frametime-app/Cargo.toml"
cli="$root/crates/frametime-cli/Cargo.toml"
gui="$root/crates/frametime-gui/Cargo.toml"

manifest_has_path "$windows" frametime-domain || fail 'architecture boundary: Windows must depend on domain'
manifest_has_path "$app" frametime-domain || fail 'architecture boundary: app must depend on domain'
manifest_has_path "$app" frametime-windows || fail 'architecture boundary: app must depend on Windows'
manifest_has_path "$cli" frametime-app || fail 'architecture boundary: CLI must depend on app'
manifest_has_path "$gui" frametime-app || fail 'architecture boundary: GUI must depend on app'

for crate in frametime-app frametime-windows; do
  matches="$(grep -RnE '\b(e?print|e?println|dbg)!' "$root/crates/$crate/src" --include='*.rs' || true)"
  [[ -z "$matches" ]] || { printf 'architecture boundary: console output in %s\n%s\n' "$crate" "$matches" >&2; failed=1; }
done

for frontend in frametime-cli frametime-gui; do
  while IFS= read -r source; do
    case "${source#$root/crates/$frontend/src/}" in
      main.rs) continue ;;
    esac
    matches="$(grep -nE 'frametime_windows::|use[[:space:]]+frametime_windows' "$source" || true)"
    if [[ -n "$matches" ]]; then
      printf 'architecture boundary: %s bypasses the app boundary: %s\n%s\n' "$frontend" "${source#$root/}" "$matches" >&2
      failed=1
    fi
  done < <(find "$root/crates/$frontend/src" -type f -name '*.rs' | sort)
done

matches="$(find "$root/crates/frametime-gui/src" -type f -name '*.rs' -exec sed '/^#\[cfg(test)\]/,$d' {} + | grep -nE '[/][[:space:]]*(38|3|13)([^0-9]|$)' || true)"
if [[ -n "$matches" ]]; then
  printf 'architecture boundary: GUI hardcodes phase totals\n%s\n' "$matches" >&2
  failed=1
fi

exit "$failed"
