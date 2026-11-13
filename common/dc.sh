#!/usr/bin/env bash
# Atalho: common/dc.sh <trivia|accounting> <args do docker compose>
# Ex.: common/dc.sh trivia up -d
set -euo pipefail
P="${1:?uso: dc.sh <trivia|accounting> <args do docker compose>}"
shift
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENVF="$ROOT/products/$P/.env"
[ -f "$ENVF" ] || {
  echo "falta $ENVF (copie de .env.example)" >&2
  exit 1
}
exec docker compose \
  -p "kodoo-$P" \
  --project-directory "$ROOT/products/$P" \
  --env-file "$ENVF" \
  -f "$ROOT/common/compose.base.yaml" \
  "$@"
