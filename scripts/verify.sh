#!/usr/bin/env bash
# Deterministic check for cleanX. Rerun after any change.
set -uo pipefail
cd "$(dirname "$0")/.."

RUSTUP=${RUSTUP:-$(command -v rustup)}
CARGO=(bash -c 'exec -a cargo "$0" "$@"' "$RUSTUP")
OUT=${1:-/tmp/opencode/cleanx-verify.txt}
SHOTS=/tmp/opencode/cleanx-shots
mkdir -p "$SHOTS"
: > "$OUT"

status=0
step() {
  echo "== $1" >> "$OUT"
}

step "fmt"
"${CARGO[@]}" fmt -- --check >> "$OUT" 2>&1
fmt_rc=$?
echo "fmt_exit=$fmt_rc" >> "$OUT"
[ "$fmt_rc" -eq 0 ] || status=1

step "test"
"${CARGO[@]}" test >> "$OUT" 2>&1
test_rc=$?
echo "test_exit=$test_rc" >> "$OUT"
[ "$test_rc" -eq 0 ] || status=1

step "scenes"
for scene in default clean confirm burst burst-control undo empty low; do
  if [ "$scene" = default ]; then
    "${CARGO[@]}" run -- --snap "$SHOTS/default.png" >> "$OUT" 2>&1
  elif [ "$scene" = burst ] || [ "$scene" = burst-control ]; then
    "${CARGO[@]}" run -- --snap "$SHOTS/$scene.png" --scene "$scene" --at 0.35 >> "$OUT" 2>&1
  else
    "${CARGO[@]}" run -- --snap "$SHOTS/$scene.png" --scene "$scene" >> "$OUT" 2>&1
  fi
  rc=$?
  size=$(wc -c < "$SHOTS/$scene.png" 2>/dev/null || echo 0)
  echo "$scene exit=$rc bytes=$size" >> "$OUT"
  [ "$rc" -eq 0 ] && [ "$size" -gt 10000 ] || status=1
done

step "scene difference"
if cmp -s "$SHOTS/burst.png" "$SHOTS/burst-control.png"; then
  echo "burst_matches_control=1" >> "$OUT"
  status=1
else
  echo "burst_matches_control=0" >> "$OUT"
fi
shots=("$SHOTS/default.png" "$SHOTS/clean.png" "$SHOTS/confirm.png" "$SHOTS/burst.png" "$SHOTS/burst-control.png" "$SHOTS/undo.png" "$SHOTS/empty.png" "$SHOTS/low.png")
same=0
for ((i = 0; i < ${#shots[@]}; i++)); do
  for ((j = i + 1; j < ${#shots[@]}; j++)); do
    if cmp -s "${shots[$i]}" "${shots[$j]}"; then
      echo "identical=${shots[$i]},${shots[$j]}" >> "$OUT"
      same=1
      status=1
    fi
  done
done
echo "any_scenes_identical=$same" >> "$OUT"

echo "OVERALL=$([ "$status" -eq 0 ] && echo PASS || echo FAIL)" >> "$OUT"
cat "$OUT"
exit "$status"
