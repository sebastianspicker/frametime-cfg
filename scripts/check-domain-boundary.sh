#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
domain="$root/crates/frametime-domain"
pattern='std::(fs|env|process)|\bfs::|\bwindows::|std::os::windows|SystemTime::now|Instant::now|OffsetDateTime::now|std::process::id|\btime[[:space:]]*='
failed=0

while IFS= read -r source; do
  # Every domain module keeps its tests in a terminal cfg(test) block. Do not
  # treat test fixtures as production boundary violations.
  production="$(sed '/^#\[cfg(test)\]/,$d; /^#\[cfg(any())\]/,$d' "$source")"
  if matches="$(printf '%s\n' "$production" | grep -nE "$pattern" || true)"; then
    if [[ -n "$matches" ]]; then
      printf '%s\n%s\n' "domain boundary violation: ${source#$root/}" "$matches" >&2
      failed=1
    fi
  fi
done < <(find "$domain/src" -type f -name '*.rs' | sort)

if grep -nE '^(time|windows)([[:space:]]|\.)*=' "$domain/Cargo.toml"; then
  echo 'domain boundary violation: platform or clock dependency in Cargo.toml' >&2
  failed=1
fi

exit "$failed"
