#!/usr/bin/env bash
set -euo pipefail

# Pinned laravel/framework revision (13.x). The frameworks test suite's
# expect! snapshots and the benchmark corpus are tied to this exact tree —
# bump the pin and refresh the snapshots (UPDATE_EXPECT=1) together.
LARAVEL_REV="8f07efbebf13"

FIXTURE_DIR="$(dirname "$0")/../benches/fixtures/laravel"

if [ -f "$FIXTURE_DIR/.rev" ] && [ "$(cat "$FIXTURE_DIR/.rev")" = "$LARAVEL_REV" ]; then
  echo "Laravel fixture already present at $FIXTURE_DIR (rev $LARAVEL_REV)"
  exit 0
fi

echo "Fetching laravel/framework@$LARAVEL_REV into $FIXTURE_DIR ..."
rm -rf "$FIXTURE_DIR"
mkdir -p "$FIXTURE_DIR"
curl -sL "https://github.com/laravel/framework/archive/$LARAVEL_REV.tar.gz" \
  | tar xz -C "$FIXTURE_DIR" --strip-components=1
echo "$LARAVEL_REV" > "$FIXTURE_DIR/.rev"
echo "Done. $(find "$FIXTURE_DIR/src" -name '*.php' | wc -l | tr -d ' ') PHP files available."
