#!/usr/bin/env bash
# Tests for sync-upstream.sh against throwaway local repositories (no network, no GitHub).
#
# Usage: .github/scripts/test-sync-upstream.sh        (needs git and cargo)
#
# Each case builds a fake original repository ("upstream"), a fork of it ("origin") and a clone of
# the fork ("work"), makes commits on both sides, runs the script and checks the result.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$HERE/sync-upstream.sh"
REAL_GIT="$(command -v git)"
TMP="$(mktemp -d)"
if [ -n "${KEEP_TMP:-}" ]; then echo "keeping $TMP"; else trap 'rm -rf "$TMP"' EXIT; fi

export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.com GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.com
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
unset GITHUB_REPOSITORY GITHUB_OUTPUT REPORT_FILE GH_TOKEN GITHUB_TOKEN UPSTREAM_BRANCH

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); printf '  ok   %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf '  FAIL %s\n' "$1"; }
check() { # check DESCRIPTION COMMAND...
  local d="$1"
  shift
  if "$@" >/dev/null 2>&1; then ok "$d"; else bad "$d"; fi
}
contains() { grep -qF -- "$2" "$1"; }
lacks() { ! grep -qF -- "$2" "$1"; }

# A fresh trio of repositories. Upstream starts with a small Cargo workspace, docs and a workflow.
new_world() {
  W="$TMP/$1"
  rm -rf "$W"
  mkdir -p "$W"
  UP="$W/up.git"
  ORIGIN="$W/origin.git"
  WORK="$W/work"
  local seed="$W/seed"
  git init -q -b main "$seed"
  (
    cd "$seed"
    mkdir -p crates/a/src .github/workflows docs
    printf '[workspace]\nmembers = ["crates/*"]\nresolver = "2"\n\n[workspace.package]\nrepository = "file://%s"\n' "$UP" >Cargo.toml
    printf '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2021"\n' >crates/a/Cargo.toml
    printf 'pub fn one() -> u32 {\n    1\n}\n' >crates/a/src/lib.rs
    printf '# Readme\n' >README.md
    printf '| date | note |\n|---|---|\n| 2026-10-01 | first |\n' >ROADMAP.md
    printf '# Gaps\n\nline one\n' >docs/gaps.md
    printf 'checklist 1\n' >docs/parity-checklist.md
    printf 'name: CI\non: push\n' >.github/workflows/ci.yml
    cargo generate-lockfile --offline >/dev/null 2>&1
    git add -A
    git commit -q -m seed
  )
  git clone -q --bare "$seed" "$UP"
  git clone -q --bare "$UP" "$ORIGIN"
  git clone -q "$ORIGIN" "$WORK"
  export UPSTREAM_REPO="file://$UP"
}

# Commit in a scratch clone of a bare repository and push: commit REPO FILE CONTENT MESSAGE
side_commit() {
  local repo="$1" file="$2" content="$3" msg="$4" c
  c="$(mktemp -d "$TMP/side.XXXXXX")"
  git clone -q "$repo" "$c"
  (
    cd "$c"
    mkdir -p "$(dirname "$file")"
    printf '%b' "$content" >"$file"
    git add -A
    git commit -q -m "$msg"
    git push -q origin HEAD:main
  )
  rm -rf "$c"
}

# Commit in the work clone (not pushed): work_commit FILE CONTENT MESSAGE
work_commit() {
  (
    cd "$WORK"
    mkdir -p "$(dirname "$1")"
    printf '%b' "$2" >"$1"
    git add -A
    git commit -q -m "$3"
  )
}

run_sync() { # run_sync [args]: runs in WORK, captures the status and the report
  REPORT="$W/report.md"
  OUT="$W/out.txt"
  rm -f "$REPORT" "$OUT"
  RC=0
  (cd "$WORK" && REPORT_FILE="$REPORT" GITHUB_OUTPUT="$OUT" "$SCRIPT" "$@" >"$W/log.txt" 2>&1) || RC=$?
  STATUS="$(sed -n 's/^status=//p' "$OUT" 2>/dev/null | tail -n 1)"
}

tip() { git -C "$1" rev-parse "${2:-HEAD}"; }
parents() { git -C "$1" rev-list --parents -n 1 "${2:-HEAD}" | wc -w; }

echo "up to date"
new_world uptodate
run_sync --push
check "status up-to-date, exit 0" test "$STATUS" = up-to-date -a "$RC" = 0
check "report says so" contains "$REPORT" "is up to date"

echo "fast forward"
new_world ff
side_commit "$UP" docs/new.md 'new\n' "upstream adds a file"
run_sync --push
check "fast-forwarded" test "$STATUS" = fast-forwarded -a "$RC" = 0
check "work has the file" test -f "$WORK/docs/new.md"
check "origin has the tip" test "$(tip "$ORIGIN" main)" = "$(tip "$UP" main)"

echo "clean merge"
new_world merge
work_commit crates/a/src/extra.rs 'pub fn two() {}\n' "fork adds a file"
side_commit "$UP" crates/a/src/other.rs 'pub fn three() {}\n' "upstream adds another"
run_sync --push
check "merged" test "$STATUS" = merged -a "$RC" = 0
check "merge commit has two parents" test "$(parents "$WORK")" = 3
check "both files present" test -f "$WORK/crates/a/src/extra.rs" -a -f "$WORK/crates/a/src/other.rs"
check "pushed to origin" test "$(tip "$ORIGIN" main)" = "$(tip "$WORK")"
check "the merge commit is by the bot only when no identity is set (here the test identity)" test -n "$(git -C "$WORK" log -1 --format=%an)"

echo "docs: both sides' lines are kept"
new_world docs
work_commit ROADMAP.md '| date | note |\n|---|---|\n| 2026-10-13 | fork row |\n| 2026-10-01 | first |\n' "fork row"
side_commit "$UP" ROADMAP.md '| date | note |\n|---|---|\n| 2026-10-12 | upstream row |\n| 2026-10-01 | first |\n' "upstream row"
work_commit docs/gaps.md '# Gaps\n\nline one (fork)\n' "fork edits a doc line"
side_commit "$UP" docs/gaps.md '# Gaps\n\nline one (upstream)\n' "upstream edits the same doc line"
run_sync --push
check "merged without stopping" test "$STATUS" = merged -a "$RC" = 0
check "both roadmap rows kept" bash -c "grep -q 'fork row' '$WORK/ROADMAP.md' && grep -q 'upstream row' '$WORK/ROADMAP.md'"
check "no conflict markers" bash -c "! grep -rq '^<<<<<<<' '$WORK/ROADMAP.md' '$WORK/docs/gaps.md'"
check "the temporary attributes are gone" test ! -s "$WORK/.git/info/attributes"

echo "generated doc: upstream's"
new_world gen
work_commit docs/parity-checklist.md 'checklist fork\n' "fork edits the generated file"
side_commit "$UP" docs/parity-checklist.md 'checklist upstream\n' "upstream regenerates it"
run_sync --push
check "merged" test "$STATUS" = merged
check "upstream's checklist" contains "$WORK/docs/parity-checklist.md" "checklist upstream"

echo "source conflict: stop and leave everything as it was"
new_world conflict
work_commit crates/a/src/lib.rs 'pub fn one() -> u32 {\n    11\n}\n' "fork changes the line"
side_commit "$UP" crates/a/src/lib.rs 'pub fn one() -> u32 {\n    111\n}\n' "upstream changes the line"
before="$(tip "$WORK")"
origin_before="$(tip "$ORIGIN" main)"
run_sync --push
check "status conflict, exit 1" test "$STATUS" = conflict -a "$RC" = 1
check "work branch unchanged" test "$(tip "$WORK")" = "$before"
check "no merge in progress" test ! -e "$WORK/.git/MERGE_HEAD"
check "work tree clean" test -z "$(git -C "$WORK" status --porcelain)"
check "origin unchanged" test "$(tip "$ORIGIN" main)" = "$origin_before"
check "report names the file" contains "$REPORT" "crates/a/src/lib.rs"

echo "workflow files: the fork keeps its own"
new_world workflows
mkdir -p "$WORK/.github/workflows"
work_commit .github/workflows/android.yml 'name: Android\non: workflow_dispatch\n' "fork workflow"
side_commit "$UP" .github/workflows/ci.yml 'name: CI\non: [push, pull_request]\n' "upstream edits ci"
side_commit "$UP" .github/workflows/new.yml 'name: New\non: push\n' "upstream adds new.yml"
side_commit "$UP" crates/a/src/other.rs 'pub fn three() {}\n' "upstream real change"
ci_before="$(cat "$WORK/.github/workflows/ci.yml")"
run_sync --push
check "merged" test "$STATUS" = merged -a "$RC" = 0
check "ci.yml as before" test "$(cat "$WORK/.github/workflows/ci.yml")" = "$ci_before"
check "new.yml not taken" test ! -e "$WORK/.github/workflows/new.yml"
check "android.yml kept" test -f "$WORK/.github/workflows/android.yml"
check "the real change came" test -f "$WORK/crates/a/src/other.rs"
check "upstream is a parent of the merge" git -C "$WORK" merge-base --is-ancestor "$(tip "$UP" main)" HEAD
check "report says workflows were kept" contains "$REPORT" ".github/workflows"
# Upstream touches the same workflow again later: still no stop.
side_commit "$UP" .github/workflows/ci.yml 'name: CI\non: [push, pull_request, workflow_dispatch]\n' "upstream edits ci again"
side_commit "$UP" crates/a/src/more.rs 'pub fn four() {}\n' "and a real change"
run_sync --push
check "a second sync also merges" test "$STATUS" = merged -a "$RC" = 0
check "ci.yml still as before" test "$(cat "$WORK/.github/workflows/ci.yml")" = "$ci_before"

echo "Cargo.lock conflict: settled with cargo"
new_world lock
mkdir -p "$WORK"
(
  cd "$WORK"
  mkdir -p crates/x1/src
  printf '[package]\nname = "x1"\nversion = "0.1.0"\nedition = "2021"\n' >crates/x1/Cargo.toml
  : >crates/x1/src/lib.rs
  cargo generate-lockfile --offline >/dev/null 2>&1
  git add -A
  git commit -q -m "fork adds crate x1"
)
c="$(mktemp -d "$TMP/side.XXXXXX")"
git clone -q "$UP" "$c"
(
  cd "$c"
  mkdir -p crates/x2/src
  printf '[package]\nname = "x2"\nversion = "0.1.0"\nedition = "2021"\n' >crates/x2/Cargo.toml
  : >crates/x2/src/lib.rs
  cargo generate-lockfile --offline >/dev/null 2>&1
  git add -A
  git commit -q -m "upstream adds crate x2"
  git push -q origin HEAD:main
)
rm -rf "$c"
run_sync --push
check "merged" test "$STATUS" = merged -a "$RC" = 0
check "the lock names both crates" bash -c "grep -q 'name = \"x1\"' '$WORK/Cargo.lock' && grep -q 'name = \"x2\"' '$WORK/Cargo.lock'"
check "and is consistent (--locked works)" bash -c "cd '$WORK' && cargo metadata --locked --offline --format-version 1 >/dev/null 2>&1"
check "report mentions the lock" contains "$REPORT" "Cargo.lock"

echo "a merge that does not resolve is undone"
new_world broken
work_commit crates/a/Cargo.toml '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nnot-a-real-crate-zzz = { path = "../nowhere" }\n' "fork breaks the manifest"
side_commit "$UP" crates/a/src/other.rs 'pub fn three() {}\n' "upstream"
side_commit "$UP" crates/a/Cargo.toml '[package]\nname = "a"\nversion = "0.2.0"\nedition = "2021"\n' "upstream bumps the version"
before="$(tip "$WORK")"
run_sync --push
check "error (the merged workspace does not resolve), nothing pushed" test "$STATUS" = error -a "$RC" = 1
check "work unchanged" test "$(tip "$WORK")" = "$before"

echo "origin refuses workflow changes"
new_world refuse
work_commit crates/a/src/extra.rs 'pub fn two() {}\n' "fork commit"
git -C "$WORK" push -q origin HEAD:main # as in CI: the checkout is origin's tip
side_commit "$UP" crates/a/src/other.rs 'pub fn three() {}\n' "upstream"
# shellcheck disable=SC2016 # the backticks are part of GitHub's message
printf '#!/bin/sh\necho "remote: refusing to allow a GitHub App to create or update workflow `.github/workflows/x.yml` without `workflows` permission" >&2\nexit 1\n' >"$ORIGIN/hooks/pre-receive"
chmod +x "$ORIGIN/hooks/pre-receive"
run_sync --push
check "push-needs-token, exit 1" test "$STATUS" = push-needs-token -a "$RC" = 1
check "report explains the secret" contains "$REPORT" "SYNC_TOKEN"
check "the output sha is the branch as it is on origin" test "$(sed -n 's/^sha=//p' "$OUT" | tail -n 1)" = "$(tip "$ORIGIN" main)"
git -C "$WORK" reset -q --hard "$(tip "$ORIGIN" main)"
side_commit "$UP" crates/a/src/other2.rs 'pub fn four() {}\n' "upstream again"
SYNC_TOKEN_IN_USE=1 run_sync --push
check "with SYNC_TOKEN set the report says the token lacks permission" contains "$REPORT" "SYNC_TOKEN is set, but GitHub still refused"

echo "the branch moves while syncing"
new_world race
work_commit crates/a/src/extra.rs 'pub fn two() {}\n' "fork commit"
git -C "$WORK" push -q origin HEAD:main # as in CI: the checkout is origin's tip
side_commit "$UP" crates/a/src/other.rs 'pub fn three() {}\n' "upstream"
SHIM="$W/shim"
mkdir -p "$SHIM"
cat >"$SHIM/git" <<EOF
#!/bin/bash
# On the first push, someone else pushes to origin first.
if [ "\$1" = push ] && [ ! -f "$W/raced" ]; then
  touch "$W/raced"
  c="\$(mktemp -d)"
  "$REAL_GIT" clone -q "$ORIGIN" "\$c"
  (cd "\$c" && printf 'raced\n' >raced.txt && "$REAL_GIT" add -A && "$REAL_GIT" commit -q -m raced && "$REAL_GIT" push -q origin HEAD:main)
  rm -rf "\$c"
fi
exec "$REAL_GIT" "\$@"
EOF
chmod +x "$SHIM/git"
PATH="$SHIM:$PATH" run_sync --push
check "merged after trying again" test "$STATUS" = merged -a "$RC" = 0
check "origin has the racing commit and the merge" bash -c "$REAL_GIT -C '$ORIGIN' show main:raced.txt >/dev/null && $REAL_GIT -C '$ORIGIN' show main:crates/a/src/other.rs >/dev/null"
check "origin is the work tip" test "$(tip "$ORIGIN" main)" = "$(tip "$WORK")"

echo "uncommitted changes are refused"
new_world dirty
side_commit "$UP" docs/new.md 'new\n' "upstream"
printf 'edit\n' >>"$WORK/README.md"
run_sync --push
check "error, exit 1" test "$STATUS" = error -a "$RC" = 1
check "the edit is still there" contains "$WORK/README.md" "edit"

echo "no --push: the merge stays local"
new_world local
work_commit crates/a/src/extra.rs 'pub fn two() {}\n' "fork commit"
side_commit "$UP" crates/a/src/other.rs 'pub fn three() {}\n' "upstream"
origin_before="$(tip "$ORIGIN" main)"
run_sync
check "merged locally" test "$STATUS" = merged -a "$RC" = 0
check "origin untouched" test "$(tip "$ORIGIN" main)" = "$origin_before"

echo "the original repository itself"
new_world original
export UPSTREAM_REPO="file://$ORIGIN"
run_sync --push
check "nothing to do" test "$STATUS" = up-to-date -a "$RC" = 0

echo "a pure mirror does not fast forward over workflow changes"
new_world mirror
side_commit "$UP" .github/workflows/ci.yml 'name: CI\non: [push, pull_request]\n' "upstream edits ci"
side_commit "$UP" crates/a/src/other.rs 'pub fn three() {}\n' "upstream real change"
ci_before="$(cat "$WORK/.github/workflows/ci.yml")"
run_sync --push
check "merged with a commit, not fast-forwarded" test "$STATUS" = merged -a "$RC" = 0
check "ci.yml as before" test "$(cat "$WORK/.github/workflows/ci.yml")" = "$ci_before"

echo "workflows the fork deleted stay deleted"
new_world deleted
git -C "$WORK" rm -q .github/workflows/ci.yml
git -C "$WORK" commit -q -m "fork deletes the upstream workflow"
work_commit .github/workflows/android.yml 'name: Android\non: workflow_dispatch\n' "fork workflow"
side_commit "$UP" .github/workflows/ci.yml 'name: CI\non: [push, pull_request]\n' "upstream edits ci"
side_commit "$UP" crates/a/src/other.rs 'pub fn three() {}\n' "upstream real change"
run_sync --push
check "merged (a modify/delete conflict in workflows is not a stop)" test "$STATUS" = merged -a "$RC" = 0
check "ci.yml still deleted" test ! -e "$WORK/.github/workflows/ci.yml"
check "android.yml kept" test -f "$WORK/.github/workflows/android.yml"

echo "status lines: upstream's one is kept"
new_world status
side_commit "$UP" docs/gaps.md '> **Last reviewed:** 2026-10-01 · **Last updated:** 2026-10-01\n\n# Gaps\n\nline one\n' "give the doc a status line"
git -C "$WORK" pull -q origin main
work_commit docs/gaps.md '> **Last reviewed:** 2026-10-01 · **Last updated:** 2026-10-13 · **Change:** fork\n\n# Gaps\n\nline one\n' "fork bumps the status line"
side_commit "$UP" docs/gaps.md '> **Last reviewed:** 2026-10-01 · **Last updated:** 2026-10-12 · **Change:** upstream\n\n# Gaps\n\nline one\n' "upstream bumps the status line"
run_sync --push
check "merged" test "$STATUS" = merged -a "$RC" = 0
check "one status line" test "$(grep -c '^> \*\*Last reviewed' "$WORK/docs/gaps.md")" = 1
check "and it is upstream's" contains "$WORK/docs/gaps.md" "Change:** upstream"

echo "--keep-conflicts leaves the merge to finish by hand"
new_world keep
mkdir -p "$WORK/.github/workflows"
work_commit .github/workflows/android.yml 'name: Android\non: workflow_dispatch\n' "fork workflow"
work_commit crates/a/src/lib.rs 'pub fn one() -> u32 {\n    11\n}\n' "fork changes the line"
work_commit ROADMAP.md '| date | note |\n|---|---|\n| 2026-10-13 | fork row |\n| 2026-10-01 | first |\n' "fork row"
side_commit "$UP" crates/a/src/lib.rs 'pub fn one() -> u32 {\n    111\n}\n' "upstream changes the line"
side_commit "$UP" ROADMAP.md '| date | note |\n|---|---|\n| 2026-10-12 | upstream row |\n| 2026-10-01 | first |\n' "upstream row"
side_commit "$UP" .github/workflows/new.yml 'name: New\non: push\n' "upstream adds new.yml"
run_sync --keep-conflicts
check "status conflict, exit 1" test "$STATUS" = conflict -a "$RC" = 1
check "the merge is still in progress" test -e "$WORK/.git/MERGE_HEAD"
check "only the source file is unmerged" test "$(git -C "$WORK" diff --name-only --diff-filter=U)" = "crates/a/src/lib.rs"
check "docs were united" bash -c "grep -q 'fork row' '$WORK/ROADMAP.md' && grep -q 'upstream row' '$WORK/ROADMAP.md'"
check "upstream's new workflow is not there" test ! -e "$WORK/.github/workflows/new.yml"
check "the report names the command" contains "$REPORT" "--keep-conflicts"
# Finishing by hand works.
printf 'pub fn one() -> u32 {\n    1111\n}\n' >"$WORK/crates/a/src/lib.rs"
check "git add and commit finish it" bash -c "cd '$WORK' && git add -A && git commit -q -m done && test ! -e .git/MERGE_HEAD"

echo "upstream cannot run code in the sync job through cargo configuration"
new_world hostile
side_commit "$UP" .cargo/config.toml "[build]\nrustc-wrapper = \"$W/evil.sh\"\n" "upstream adds a cargo config with a rustc wrapper"
printf '#!/bin/sh\ntouch "%s/pwned"\nexec "$@"\n' "$W" >"$W/evil.sh"
chmod +x "$W/evil.sh"
side_commit "$UP" crates/a/Cargo.toml '[package]\nname = "a"\nversion = "0.2.0"\nedition = "2021"\n' "upstream bumps a version"
work_commit crates/a/src/extra.rs 'pub fn two() {}\n' "fork commit"
run_sync --push
check "merged" test "$STATUS" = merged -a "$RC" = 0
check "the wrapper never ran" test ! -e "$W/pwned"

echo "a workspace cargo cannot resolve is an error with cargo's message, not a conflict"
new_world unresolved
side_commit "$UP" crates/a/Cargo.toml '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nnot-there = { path = "../nowhere" }\n' "upstream adds a dependency that does not exist"
work_commit crates/a/src/extra.rs 'pub fn two() {}\n' "fork commit"
before="$(tip "$WORK")"
run_sync --push
check "status error, exit 1" test "$STATUS" = error -a "$RC" = 1
check "branch unchanged, nothing pushed" test "$(tip "$WORK")" = "$before"
check "the report has cargo's message" contains "$REPORT" "cargo cannot resolve"

echo
echo "passed: $PASS, failed: $FAIL"
[ "$FAIL" = 0 ]
