#!/usr/bin/env bash

# Print whether the full native Rust CI jobs are required for a Git change.
# Any input, inventory, or diff condition this classifier does not understand
# fails safe to the full suite.

set -u
set -o pipefail

event_name=${1-}
base_sha=${2-}
head_sha=${3-}
inventory=${4-package-layout.txt}

full() {
  printf 'full=true\n'
  exit 0
}

is_commit_sha() {
  value=$1
  test "${#value}" -eq 40 || return 1
  case "$value" in
    *[!0-9a-fA-F]*) return 1 ;;
  esac
}

case "$event_name" in
  push | pull_request) ;;
  workflow_dispatch | *) full ;;
esac

is_commit_sha "$base_sha" || full
is_commit_sha "$head_sha" || full
git cat-file -e "${base_sha}^{commit}" 2>/dev/null || full
git cat-file -e "${head_sha}^{commit}" 2>/dev/null || full

test -f "$inventory" || full
test ! -L "$inventory" || full
test -r "$inventory" || full
inventory_bytes=$(LC_ALL=C wc -c < "$inventory") || full
inventory_without_nul_bytes=$(LC_ALL=C tr -d '\000' < "$inventory" | wc -c) || full
test "$inventory_bytes" = "$inventory_without_nul_bytes" || full

inventory_paths=()
inventory_folded=()
while IFS= read -r path || test -n "$path"; do
  test -n "$path" || full
  case "$path" in
    /* | */ | *//* | *\\* | *:*) full ;;
  esac
  LC_ALL=C expr "$path" : '.*[[:cntrl:]]' >/dev/null && full
  case "/$path/" in
    */./* | */../*) full ;;
  esac
  folded=$(printf '%s' "$path" | LC_ALL=C tr '[:upper:]' '[:lower:]') || full
  for known in "${inventory_folded[@]}"; do
    test "$folded" != "$known" || full
  done
  inventory_paths+=("$path")
  inventory_folded+=("$folded")
done < "$inventory"
test "${#inventory_paths[@]}" -gt 0 || full

diff_file=$(mktemp "${TMPDIR:-/tmp}/frametime-ci-diff.XXXXXX") || full
trap 'rm -f "$diff_file"' EXIT
if ! git diff --name-status -z --find-renames --no-ext-diff \
  "$base_sha" "$head_sha" -- > "$diff_file"; then
  full
fi

changed_paths=()
malformed_diff=false
while IFS= read -r -d '' status; do
  case "$status" in
    A | D | M | T | U | X | B)
      if IFS= read -r -d '' path; then
        changed_paths+=("$path")
      else
        malformed_diff=true
        break
      fi
      ;;
    *)
      if [[ "$status" =~ ^[RC][0-9]{1,3}$ ]] \
        && IFS= read -r -d '' old_path \
        && IFS= read -r -d '' new_path; then
        changed_paths+=("$old_path" "$new_path")
      else
        malformed_diff=true
        break
      fi
      ;;
  esac
done < "$diff_file"

test "$malformed_diff" = false || full
test "${#changed_paths[@]}" -gt 0 || full

for path in "${changed_paths[@]}"; do
  test -n "$path" || full
  test "$path" != "package-layout.txt" || full

  folded=$(printf '%s' "$path" | LC_ALL=C tr '[:upper:]' '[:lower:]') || full
  for index in "${!inventory_paths[@]}"; do
    test "$folded" != "${inventory_folded[$index]}" || full
  done

  case "$path" in
    CHANGELOG.md | CODE_OF_CONDUCT.md | CONTRIBUTING.md | README.md) continue ;;
    docs/*.md) continue ;;
    # Static documentation media and the GitHub Pages demo carry no Rust code
    # and cannot change the native gate.
    docs/assets/*) continue ;;
    # The Pages build regenerates this executable data from the Rust catalog
    # and planner behavior, then compares it with the checked-in copy.
    site/register.js) full ;;
    site/*) continue ;;
    .github/ISSUE_TEMPLATE/*.md | .github/ISSUE_TEMPLATE/*.yml | .github/ISSUE_TEMPLATE/*.yaml) continue ;;
    .github/PULL_REQUEST_TEMPLATE.md | .github/PULL_REQUEST_TEMPLATE/*.md) continue ;;
  esac
  full
done

printf 'full=false\n'
