#!/usr/bin/env bash
# Build/test a BESS checkout on dev17 (24 cores) instead of this laptop.
#   tools/dev17.sh <name> <local_src_dir> <command...>
#     Syncs the checkout to dev17:~/bess-build/<name>/src (no target/.git),
#     runs <command> there with a per-name target dir, streams output.
#     Files the command writes under ./out/ come back to <local_src_dir>/../dev17-out/<name>/
#     (or to $DEV17_PULL if set).
#   Example: tools/dev17.sh main "$PWD" cargo test --release
#            tools/dev17.sh main "$PWD" cargo run --release --example final_proof -- out/after
# bdsp (private GitHub) is not reachable from dev17: the pinned local checkout
# is synced and patched in by path.
set -euo pipefail
name=$1; src=$(realpath "$2"); shift 2
host=dev17; base="bess-build/$name"
# The bdsp revision pinned in Cargo.lock, from the local cargo git checkout.
rev=$(grep -o 'bunchyearth23/bdsp?rev=[0-9a-f]*' "$src/Cargo.lock" | head -1 | cut -d= -f2)
bdsp=$(ls -d ~/.cargo/git/checkouts/bdsp-*/"${rev:0:7}" | head -1)
[ -d "$bdsp" ] || { echo "bdsp $rev not checked out locally: run cargo fetch" >&2; exit 1; }
# ponytail: one shared remote bdsp copy; per-rev dirs if checkouts ever pin different revs.
ssh -o BatchMode=yes $host "mkdir -p ~/bess-build/bdsp ~/$base/src/.cargo"
rsync -a --delete "$bdsp/" "$host:bess-build/bdsp/"
rsync -a --delete --exclude target --exclude .git --exclude .claude --exclude out \
  --exclude .cargo "$src/" "$host:$base/src/"
# The checkout's own cargo config (e.g. rustflags) plus the bdsp path patch.
{ cat "$src/.cargo/config.toml" 2>/dev/null; printf '\n[patch."ssh://git@github.com/bunchyearth23/bdsp"]\nbdsp = { path = "/home/manager/bess-build/bdsp" }\n'; } |
  ssh -o BatchMode=yes $host "cat > ~/$base/src/.cargo/config.toml"
set +e
ssh -o BatchMode=yes $host "cd ~/$base/src && rm -rf out && mkdir -p out && CARGO_TARGET_DIR=~/$base/target $(printf '%q ' "$@")"
status=$?
set -e
pull=${DEV17_PULL:-$(dirname "$src")/dev17-out/$name}
if ssh -o BatchMode=yes $host "test -n \"\$(ls -A ~/$base/src/out 2>/dev/null)\""; then
  mkdir -p "$pull"; rsync -a "$host:$base/src/out/" "$pull/"; echo "[dev17] outputs -> $pull"
fi
exit $status
