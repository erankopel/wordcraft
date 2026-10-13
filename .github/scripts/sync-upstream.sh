#!/usr/bin/env bash
# Merge the original repository's newest commits into a branch of this fork.
#
# Usage: .github/scripts/sync-upstream.sh [--push] [--branch NAME]
#
#   --push         push the result to origin (without it the merge stays in your local checkout)
#   --branch NAME  the branch to bring up to date (default: the current branch)
#
# The original repository is, in order: $UPSTREAM_REPO (owner/name or a git URL), the parent of this
# repository on GitHub (when `gh` is logged in and $GITHUB_REPOSITORY is set), the repository field
# of Cargo.toml. Its default branch is merged, or $UPSTREAM_BRANCH.
#
# What it does:
#   * nothing when the branch has everything already;
#   * a fast forward when the branch has no commits of its own;
#   * otherwise a merge commit, never a rebase and never a force push, so nothing of yours is lost.
#     A merge that conflicts in source code is undone and reported: the branch is left as it was.
#
# Conflicts it settles itself, because the fork's changes there are not worth stopping for:
#   * .github/workflows: the fork's own workflows are kept as they are (upstream's CI needs its
#     organisation's secrets and does nothing here). This also means a push needs no token with the
#     workflow permission.
#   * README.md, ROADMAP.md, AGENTS.md, CLAUDE.md, ATTRIBUTION.md, docs/*.md: both sides' lines are
#     kept (git's union merge), which suits the progress logs and tables that everyone appends to.
#   * docs/parity-checklist.md (generated): upstream's version.
#   * Cargo.lock: upstream's, then cargo adds what the fork's crates need. After any merge that
#     touches a Cargo.toml or the lock, cargo must still resolve the workspace, or the merge is undone.
#
# Results: the exit status (0 when the branch is up to date or was brought up to date), and, when
# set, $GITHUB_OUTPUT (status, sha, behind) and $REPORT_FILE (a Markdown report). Statuses:
# up-to-date, fast-forwarded, merged, conflict, push-needs-token, push-rejected, error.
#
# Tested against local repositories by .github/scripts/test-sync-upstream.sh.
set -euo pipefail

PUSH=0
BRANCH=""
while [ $# -gt 0 ]; do
  case "$1" in
    --push) PUSH=1 ;;
    --branch)
      [ $# -ge 2 ] || { echo "error: --branch needs a name" >&2; exit 2; }
      BRANCH="$2"
      shift
      ;;
    -h | --help) sed -n '2,/^[^#]/{/^#/p}' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "error: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done

cd "$(git rev-parse --show-toplevel)"

STATUS=""
BEHIND=0
UP_URL=""
UP_BRANCH="${UPSTREAM_BRANCH:-}"
PRE=""
NOTES=()
CONFLICTS=()
PUSH_ERR_TEXT=""
PUSH_FAILURE=""
UP="refs/sync-upstream/head"

# Where the merge's attribute overrides go, and how to put them back.
ATTRS="$(git rev-parse --git-path info/attributes)"
ATTRS_BAK=""
ATTRS_HAD=0
restore_attrs() {
  [ -n "$ATTRS_BAK" ] || return 0
  if [ "$ATTRS_HAD" = 1 ]; then cp "$ATTRS_BAK" "$ATTRS"; else rm -f "$ATTRS"; fi
  rm -f "$ATTRS_BAK"
  ATTRS_BAK=""
}
trap restore_attrs EXIT

say() { printf '%s\n' "$*"; }

# The report, for the job summary: what happened and, when it did not work, what to do.
write_report() {
  [ -n "${REPORT_FILE:-}" ] || return 0
  local branch_name="${BRANCH:-?}" short=""
  short="$(git rev-parse --short "$UP" 2>/dev/null || true)"
  {
    case "$STATUS" in
      up-to-date) echo "### \`$branch_name\` is up to date" ;;
      fast-forwarded | merged) echo "### \`$branch_name\` was brought up to date ($STATUS, $BEHIND new upstream commits)" ;;
      conflict) echo "### \`$branch_name\` could not be updated: a merge conflict" ;;
      push-needs-token) echo "### \`$branch_name\` was merged but GitHub refused the push" ;;
      push-rejected) echo "### \`$branch_name\` was merged but the push failed" ;;
      *) echo "### Syncing \`$branch_name\` failed" ;;
    esac
    echo
    [ -z "$UP_URL" ] || echo "Original repository: \`$UP_URL\` branch \`$UP_BRANCH\` ${short:+(at \`$short\`)}"
    if [ ${#NOTES[@]} -gt 0 ]; then
      echo
      printf -- '- %s\n' "${NOTES[@]}"
    fi
    case "$STATUS" in
      conflict)
        echo
        echo "These files changed on both sides and git cannot combine them:"
        echo
        # shellcheck disable=SC2016 # the backticks are Markdown
        printf -- '- `%s`\n' "${CONFLICTS[@]}"
        echo
        echo "The branch was left as it was. To merge by hand, in a clone of your fork:"
        echo
        echo '```sh'
        echo "git remote add upstream $UP_URL   # once"
        echo "git fetch upstream"
        echo "git switch $branch_name && git pull"
        echo "git merge upstream/$UP_BRANCH     # fix the files above, then git add them"
        echo "git commit && git push"
        echo '```'
        ;;
      push-needs-token)
        echo
        echo "The merge changes a workflow file and the workflow's own token may not push those. Either"
        echo "add a repository secret SYNC_TOKEN (a fine-grained personal access token for this"
        echo "repository with Contents and Workflows read and write), or use GitHub's Sync fork button"
        echo "on the repository page (not Discard commits: that would throw your changes away)."
        echo
        echo '```'
        echo "$PUSH_ERR_TEXT"
        echo '```'
        ;;
      push-rejected | error)
        if [ -n "$PUSH_ERR_TEXT" ]; then
          echo
          echo '```'
          echo "$PUSH_ERR_TEXT"
          echo '```'
        fi
        ;;
    esac
  } >"$REPORT_FILE"
}

finish() {
  STATUS="$1"
  local sha
  sha="$(git rev-parse HEAD 2>/dev/null || true)"
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    {
      echo "status=$STATUS"
      echo "sha=$sha"
      echo "behind=$BEHIND"
    } >>"$GITHUB_OUTPUT"
  fi
  write_report
  say "sync-upstream: $STATUS (branch ${BRANCH:-?} at ${sha:0:12}, $BEHIND new upstream commits)"
  case "$STATUS" in
    up-to-date | fast-forwarded | merged) exit 0 ;;
    *) exit 1 ;;
  esac
}

fail() {
  say "error: $*" >&2
  PUSH_ERR_TEXT="$*"
  finish error
}

norm() { # a repository name or URL as owner/name in lower case, to tell two spellings apart
  local s="$1"
  s="${s%/}"
  s="${s%.git}"
  case "$s" in
    *://*)
      s="${s#*://}" # drop the scheme and the host
      s="${s#*/}"
      ;;
    git@*) s="${s#*:}" ;;
  esac
  s="${s#/}"
  printf '%s' "$s" | tr '[:upper:]' '[:lower:]'
}

resolve_upstream() {
  local spec="${UPSTREAM_REPO:-}"
  if [ -z "$spec" ] && [ -n "${GITHUB_REPOSITORY:-}" ] && [ -n "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ] && command -v gh >/dev/null 2>&1; then
    spec="$(gh api "repos/$GITHUB_REPOSITORY" --jq '.parent.full_name // empty' 2>/dev/null || true)"
  fi
  if [ -z "$spec" ] && [ -f Cargo.toml ]; then
    spec="$(sed -n 's/^repository *= *"\(.*\)".*$/\1/p' Cargo.toml | head -n 1)"
  fi
  case "$spec" in
    '') fail "cannot tell which repository this fork came from: set UPSTREAM_REPO to owner/name" ;;
    *://* | git@*) UP_URL="$spec" ;;
    *) UP_URL="https://github.com/${spec%.git}.git" ;;
  esac
  if [ -z "$UP_BRANCH" ]; then
    UP_BRANCH="$(git ls-remote --symref "$UP_URL" HEAD 2>/dev/null | awk '/^ref:/ { sub("refs/heads/", "", $2); print $2; exit }')"
    UP_BRANCH="${UP_BRANCH:-main}"
  fi
}

# The tree must be clean and the branch checked out; with --push it also starts from origin's tip.
prepare() {
  if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    fail "the working tree has uncommitted changes: commit or stash them first"
  fi
  local current
  current="$(git symbolic-ref --short -q HEAD || true)"
  BRANCH="${BRANCH:-$current}"
  [ -n "$BRANCH" ] || fail "not on a branch (pass --branch NAME)"
  if [ "$current" != "$BRANCH" ]; then
    git checkout -q "$BRANCH" || fail "cannot switch to $BRANCH"
  fi
  if [ "$(git rev-parse --is-shallow-repository)" = true ]; then
    git fetch -q --unshallow --no-tags origin || fail "cannot fetch the full history"
  fi
  if [ "$PUSH" = 1 ]; then
    git fetch -q --no-tags origin "$BRANCH" || fail "cannot fetch origin $BRANCH"
    if ! git merge-base --is-ancestor HEAD FETCH_HEAD; then
      git merge-base --is-ancestor FETCH_HEAD HEAD || fail "local $BRANCH and origin/$BRANCH have diverged: update the checkout first"
    else
      git merge --ff-only -q FETCH_HEAD || fail "cannot move $BRANCH to origin's tip"
    fi
  fi
  git fetch -q --no-tags "$UP_URL" "+refs/heads/$UP_BRANCH:$UP" || fail "cannot fetch $UP_BRANCH from $UP_URL"
}

unmerged() { git diff --name-only --diff-filter=U; }

keep_own_workflows() {
  local f changed=0
  # Files upstream added there: not wanted.
  while IFS= read -r -d '' f; do
    if ! git cat-file -e "$PRE:$f" 2>/dev/null; then
      git rm -q -f --cached -- "$f" 2>/dev/null || true
      rm -f -- "$f"
      changed=1
    fi
  done < <(git ls-files -z -- .github/workflows)
  # Every other file there as it was before the merge.
  if git ls-tree -r --name-only "$PRE" -- .github/workflows | grep -q .; then
    if ! git diff --quiet "$PRE" -- .github/workflows || [ -n "$(git ls-files -u -- .github/workflows)" ]; then changed=1; fi
    git checkout -q "$PRE" -- .github/workflows
  fi
  if [ "$changed" = 1 ]; then NOTES+=("kept this fork's own .github/workflows (upstream's changes to them were not taken)"); fi
}

settle_cargo() {
  local touched=0
  if git diff --name-only "$PRE" "$UP" | grep -Eq '(^|/)Cargo\.(toml|lock)$'; then touched=1; fi
  local lock_conflict=0
  if git ls-files -u -- Cargo.lock | grep -q .; then lock_conflict=1; fi
  [ "$touched" = 1 ] || [ "$lock_conflict" = 1 ] || return 0
  if [ "$lock_conflict" = 1 ]; then
    # Only when nothing else is unmerged: the manifests must be readable to settle the lock.
    if [ "$(unmerged | grep -vc '^Cargo.lock$' || true)" != 0 ]; then return 0; fi
    git checkout -q --theirs -- Cargo.lock
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    if [ "$lock_conflict" = 1 ]; then
      NOTES+=("Cargo.lock conflicted and cargo is not installed to settle it")
      git checkout -q -m -- Cargo.lock 2>/dev/null || true
      return 0
    fi
    NOTES+=("cargo is not installed here, so the merged Cargo.lock was not checked")
    return 0
  fi
  local err before after
  err="$(mktemp)"
  before="$(git hash-object Cargo.lock 2>/dev/null || true)"
  if ! cargo metadata --format-version 1 --quiet >/dev/null 2>"$err"; then
    PUSH_ERR_TEXT="cargo cannot resolve the merged workspace: $(tail -n 5 "$err")"
    rm -f "$err"
    git merge --abort 2>/dev/null || git reset -q --hard "$PRE"
    NOTES+=("the merged workspace does not resolve with cargo")
    CONFLICTS=("Cargo.toml / Cargo.lock")
    finish conflict
  fi
  rm -f "$err"
  after="$(git hash-object Cargo.lock 2>/dev/null || true)"
  if [ "$lock_conflict" = 1 ]; then
    NOTES+=("Cargo.lock conflicted: took upstream's and let cargo add what this fork's crates need")
  elif [ "$before" != "$after" ]; then
    NOTES+=("Cargo.lock was adjusted by cargo after the merge")
  fi
  git add -- Cargo.lock
}

merge_with_policies() {
  ATTRS_BAK="$(mktemp)"
  if [ -f "$ATTRS" ]; then
    cp "$ATTRS" "$ATTRS_BAK"
    ATTRS_HAD=1
  fi
  mkdir -p "$(dirname "$ATTRS")"
  {
    echo "README.md merge=union"
    echo "ROADMAP.md merge=union"
    echo "AGENTS.md merge=union"
    echo "CLAUDE.md merge=union"
    echo "ATTRIBUTION.md merge=union"
    echo "docs/*.md merge=union"
    echo "docs/parity-checklist.md !merge"
  } >>"$ATTRS"

  local rc=0
  git merge --no-ff --no-commit -q "$UP" >/dev/null 2>&1 || rc=$?
  restore_attrs
  if [ "$rc" != 0 ] && ! git rev-parse -q --verify MERGE_HEAD >/dev/null; then
    fail "git merge failed before it could start (exit $rc)"
  fi

  keep_own_workflows
  if git ls-files -u -- docs/parity-checklist.md | grep -q .; then
    git checkout -q --theirs -- docs/parity-checklist.md
    git add -- docs/parity-checklist.md
    NOTES+=("docs/parity-checklist.md is generated: took upstream's")
  fi
  settle_cargo

  local left
  left="$(unmerged)"
  if [ -n "$left" ]; then
    while IFS= read -r f; do CONFLICTS+=("$f"); done <<<"$left"
    git merge --abort
    finish conflict
  fi

  if [ -z "$(git config user.email || true)" ]; then
    export GIT_AUTHOR_NAME="github-actions[bot]" GIT_COMMITTER_NAME="github-actions[bot]"
    export GIT_AUTHOR_EMAIL="41898282+github-actions[bot]@users.noreply.github.com" GIT_COMMITTER_EMAIL="41898282+github-actions[bot]@users.noreply.github.com"
  fi
  local body=""
  if [ ${#NOTES[@]} -gt 0 ]; then body="$(printf -- '- %s\n' "${NOTES[@]}")"; fi
  git commit -q -m "Merge $(norm "$UP_URL") $UP_BRANCH into $BRANCH ($BEHIND commits)" ${body:+-m "$body"}
  STATUS=merged
}

integrate() {
  PRE="$(git rev-parse HEAD)"
  BEHIND="$(git rev-list --count "$PRE..$UP")"
  if [ "$BEHIND" = 0 ]; then
    STATUS=up-to-date
    return 0
  fi
  if git merge-base --is-ancestor "$PRE" "$UP"; then
    git merge --ff-only -q "$UP"
    STATUS=fast-forwarded
    return 0
  fi
  merge_with_policies
}

push_branch() {
  local err
  err="$(mktemp)"
  if git push -q origin "HEAD:refs/heads/$BRANCH" 2>"$err"; then
    rm -f "$err"
    return 0
  fi
  cat "$err" >&2
  PUSH_ERR_TEXT="$(tail -n 6 "$err")"
  if grep -Eqi 'refusing to allow .*workflow|without .?workflows?.? (permission|scope)' "$err"; then
    PUSH_FAILURE=workflow
  elif grep -Eqi 'non-fast-forward|fetch first|stale info|\(fetch first\)|rejected' "$err"; then
    PUSH_FAILURE=moved
  else
    PUSH_FAILURE=other
  fi
  rm -f "$err"
  return 1
}

# --- main -------------------------------------------------------------------------------------
resolve_upstream
if [ "$(norm "$UP_URL")" = "$(norm "$(git remote get-url origin 2>/dev/null || true)")" ] ||
  { [ -n "${GITHUB_REPOSITORY:-}" ] && [ "$(norm "$UP_URL")" = "$(norm "$GITHUB_REPOSITORY")" ]; }; then
  NOTES+=("this is the original repository: there is nothing to merge from")
  BRANCH="${BRANCH:-$(git symbolic-ref --short -q HEAD || true)}"
  finish up-to-date
fi
prepare

attempt=0
while :; do
  attempt=$((attempt + 1))
  integrate
  case "$STATUS" in
    up-to-date) finish up-to-date ;;
  esac
  [ "$PUSH" = 1 ] || finish "$STATUS"
  if push_branch; then finish "$STATUS"; fi
  case "$PUSH_FAILURE" in
    workflow) finish push-needs-token ;;
    moved)
      if [ "$attempt" -ge 3 ]; then finish push-rejected; fi
      # The branch moved while this ran: undo the merge and go again from its new tip.
      say "the branch moved on origin; trying again ($attempt)"
      git reset -q --hard "$PRE"
      NOTES=()
      git fetch -q --no-tags origin "$BRANCH" || fail "cannot fetch origin $BRANCH"
      git merge --ff-only -q FETCH_HEAD || fail "$BRANCH diverged from origin while syncing"
      git fetch -q --no-tags "$UP_URL" "+refs/heads/$UP_BRANCH:$UP" || fail "cannot fetch $UP_BRANCH from $UP_URL"
      ;;
    *) finish push-rejected ;;
  esac
done
