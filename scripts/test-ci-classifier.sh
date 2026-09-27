#!/usr/bin/env bash

set -eu

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
classifier="$script_dir/classify-ci-change.sh"
fixture_root=$(mktemp -d "${TMPDIR:-/tmp}/frametime-ci-classifier.XXXXXX")
trap 'rm -rf "$fixture_root"' EXIT

case_number=0
init_case() {
  case_number=$((case_number + 1))
  case_dir="$fixture_root/case-$case_number"
  mkdir "$case_dir"
  git -C "$case_dir" init -q
  git -C "$case_dir" config user.name 'CI classifier test'
  git -C "$case_dir" config user.email 'ci-classifier@example.invalid'
  printf 'packaged.md\n' > "$case_dir/package-layout.txt"
  printf 'packaged\n' > "$case_dir/packaged.md"
  git -C "$case_dir" add .
  git -C "$case_dir" commit -qm baseline
  base_sha=$(git -C "$case_dir" rev-parse HEAD)
}

commit_case() {
  git -C "$case_dir" add -A
  git -C "$case_dir" commit -qm change
  head_sha=$(git -C "$case_dir" rev-parse HEAD)
}

assert_result() {
  expected=$1
  event=${2-pull_request}
  actual=$(cd "$case_dir" && bash "$classifier" "$event" "$base_sha" "$head_sha")
  if test "$actual" != "full=$expected"; then
    printf 'expected full=%s, got %s in %s\n' "$expected" "$actual" "$case_dir" >&2
    exit 1
  fi
}

init_case
mkdir "$case_dir/docs"
printf 'guide\n' > "$case_dir/docs/guide.md"
commit_case
assert_result false

init_case
mkdir "$case_dir/docs"
printf 'fn main() {}\n' > "$case_dir/docs/random.rs"
commit_case
assert_result true

init_case
printf 'changed\n' >> "$case_dir/packaged.md"
commit_case
assert_result true

init_case
mkdir "$case_dir/docs"
newline_path="$case_dir/docs/line
break.md"
printf 'guide\n' > "$newline_path"
commit_case
assert_result false

init_case
mkdir "$case_dir/docs"
git -C "$case_dir" mv packaged.md docs/renamed.md
commit_case
assert_result true

init_case
mkdir "$case_dir/docs"
printf 'old\n' > "$case_dir/docs/old.md"
git -C "$case_dir" add .
git -C "$case_dir" commit -qm add-doc
base_sha=$(git -C "$case_dir" rev-parse HEAD)
git -C "$case_dir" mv docs/old.md docs/new.md
commit_case
assert_result false

init_case
printf '../escape.md\n' > "$case_dir/package-layout.txt"
git -C "$case_dir" add package-layout.txt
git -C "$case_dir" commit -qm malformed-inventory
base_sha=$(git -C "$case_dir" rev-parse HEAD)
mkdir "$case_dir/docs"
printf 'guide\n' > "$case_dir/docs/guide.md"
commit_case
assert_result true

init_case
printf 'C:/absolute.md\n' > "$case_dir/package-layout.txt"
git -C "$case_dir" add package-layout.txt
git -C "$case_dir" commit -qm malformed-windows-inventory
base_sha=$(git -C "$case_dir" rev-parse HEAD)
mkdir "$case_dir/docs"
printf 'guide\n' > "$case_dir/docs/guide.md"
commit_case
assert_result true

init_case
printf 'packaged.md\0hidden.md\n' > "$case_dir/package-layout.txt"
git -C "$case_dir" add package-layout.txt
git -C "$case_dir" commit -qm nul-inventory
base_sha=$(git -C "$case_dir" rev-parse HEAD)
mkdir "$case_dir/docs"
printf 'guide\n' > "$case_dir/docs/guide.md"
commit_case
assert_result true

init_case
printf 'source\n' > "$case_dir/source.rs"
commit_case
assert_result true

init_case
printf 'README.md\n' >> "$case_dir/package-layout.txt"
commit_case
assert_result true

init_case
head_sha=$base_sha
assert_result true

init_case
mkdir "$case_dir/docs"
printf 'guide\n' > "$case_dir/docs/guide.md"
commit_case
assert_result true workflow_dispatch

init_case
head_sha=not-a-commit
assert_result true

init_case
mkdir -p "$case_dir/site"
printf '<!doctype html>\n' > "$case_dir/site/index.html"
commit_case
assert_result false

init_case
mkdir -p "$case_dir/site"
printf 'window.FRAMETIME_REGISTER = {};\n' > "$case_dir/site/register.js"
commit_case
assert_result true

init_case
mkdir -p "$case_dir/docs/assets/concepts"
printf 'pngbytes\n' > "$case_dir/docs/assets/concepts/pic.png"
commit_case
assert_result false

init_case
printf '# Code of Conduct\n' > "$case_dir/CODE_OF_CONDUCT.md"
commit_case
assert_result false

init_case
mkdir -p "$case_dir/.github/workflows"
printf 'name: Test\n' > "$case_dir/.github/workflows/test.yml"
commit_case
assert_result true

printf 'CI classifier tests: passed\n'
