#!/usr/bin/env bash
# Keeps Rust build output under a size limit. Cargo never prunes target/, so it
# only grows. This counts target/ across the main checkout and every worktree,
# and deletes in this order, stopping once the total is under the limit:
#   1. the whole target/ of other worktrees, least recently built first
#   2. the main checkout's incremental cache
#   3. the main checkout's compiled output, its release build, then the Dock app's build
# target/debug/cutaway and cutaway-app are always kept: the MCP server launches
# from the first, and a running app can carry on. The cost of a clean is a slower
# next build, nothing else. A run is skipped while cargo or rustc is working.
#
# Usage: scripts/clean-target.sh [--dry-run] [--limit <GB>]    (default 20)
set -uo pipefail

limit_gb=20
dry=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) dry=1 ;;
    --limit)
      limit_gb="${2:-}"
      shift
      ;;
    -h | --help)
      sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "clean-target: unknown option $1" >&2
      exit 2
      ;;
  esac
  shift
done
case "$limit_gb" in
  '' | *[!0-9.]*)
    echo "clean-target: --limit needs a size in GB" >&2
    exit 2
    ;;
esac

stamp() { date '+%Y-%m-%d %H:%M:%S'; }
say() { echo "$(stamp) $*"; }
fail() {
  say "failed: $*" >&2
  exit 1
}
gb() { awk -v k="$1" 'BEGIN { printf "%.1fG", k / 1048576 }'; }
kb() {
  local size
  size=$(du -sk "$1" 2>/dev/null | awk '{ print $1 }')
  echo "${size:-0}"
}
busy() { pgrep -x cargo >/dev/null 2>&1 || pgrep -x rustc >/dev/null 2>&1; }
modified() { stat -c %Y "$1" 2>/dev/null || stat -f %m "$1"; }

here="$(cd "$(dirname "$0")/.." && pwd)" || fail "can't enter the repo (macOS may be blocking access to Documents)"
worktrees="$(git -C "$here" worktree list --porcelain 2>&1)" || fail "git: $worktrees"
worktrees="$(printf '%s\n' "$worktrees" | sed -n 's/^worktree //p')"
main="$(printf '%s\n' "$worktrees" | head -1)"
# du reports an unreadable folder as small, which would read as "under the limit".
ls "$main" >/dev/null 2>&1 || fail "can't read $main (macOS may be blocking access to Documents)"

if busy; then
  say "skipped: a build is running"
  exit 0
fi

# One run at a time: the schedule and check.sh can land together.
lock="$main/.git/clean-target.lock"
if [ -d "$lock" ] && [ $(($(date +%s) - $(modified "$lock"))) -gt 3600 ]; then rmdir "$lock"; fi
if ! mkdir "$lock" 2>/dev/null; then
  say "skipped: another clean is running"
  exit 0
fi
trap 'rmdir "$lock" 2>/dev/null' EXIT

# Deleting this many files takes minutes, so folders are first moved here, which is
# instant. A build that starts during the delete finds them gone, not half gone.
grave="$main/target/.cleaning"
[ "$dry" = 1 ] || rm -rf "$grave"

measure() {
  local wt sum=0
  while IFS= read -r wt; do
    if [ -d "$wt/target" ] && [ ! -L "$wt/target" ]; then sum=$((sum + $(kb "$wt/target"))); fi
  done <<<"$worktrees"
  echo "$sum"
}
total=$(measure)
start=$total
limit_kb=$(awk -v g="$limit_gb" 'BEGIN { printf "%d", g * 1048576 }')
removed=""
over() { [ "$total" -gt "$limit_kb" ]; }

# wipe <label> <path>...: the paths go together, or not at all.
wipe() {
  local label="$1" path size=0 n=0
  shift
  for path in "$@"; do
    if [ -e "$path" ]; then size=$((size + $(kb "$path"))); fi
  done
  [ "$size" -gt 0 ] || return 0
  if busy; then
    say "stopped: a build started, leaving the rest for next time"
    finish
  fi
  removed="$removed, $label ($(gb "$size"))"
  if [ "$dry" = 1 ]; then
    # Only an estimate: folders share hard links, so their sizes overlap.
    total=$((total > size ? total - size : 0))
    return 0
  fi
  mkdir -p "$grave"
  for path in "$@"; do
    n=$((n + 1))
    if [ -e "$path" ]; then mv "$path" "$grave/$n" || fail "couldn't move $path"; fi
  done
  rm -rf "$grave"
  total=$(measure)
}

finish() {
  local what="would remove"
  [ "$dry" = 1 ] || what="removed"
  if [ -n "$removed" ]; then
    say "target $(gb "$start") -> $(gb "$total") (limit ${limit_gb}G), $what: ${removed#, }"
  else
    say "target $(gb "$start"), limit ${limit_gb}G, nothing to do"
  fi
  exit 0
}

if over; then
  while IFS= read -r line; do
    over || break
    wt="${line#* }"
    wipe "$wt/target" "$wt/target"
  done < <(
    while IFS= read -r wt; do
      if [ "$wt" != "$main" ] && [ -d "$wt/target" ] && [ ! -L "$wt/target" ]; then
        echo "$(modified "$wt/target") $wt"
      fi
    done <<<"$worktrees" | sort -n
  )
fi

debug="$main/target/debug"
if over; then wipe "incremental cache" "$debug/incremental"; fi
# Together: a fingerprint left behind would describe output that is gone.
if over; then wipe "compiled output" "$debug/deps" "$debug/build" "$debug/.fingerprint" "$debug/examples"; fi
if over; then wipe "release build" "$main/target/release"; fi
if over; then wipe "Dock app build" "$main/target/dock"; fi

finish
