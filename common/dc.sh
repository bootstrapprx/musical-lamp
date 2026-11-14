#!/usr/bin/env bash
# Atalho: common/dc.sh <produto> [--observability] <args do docker compose>
# Ex.: common/dc.sh trivia up -d
#       common/dc.sh demo --observability up -d
set -euo pipefail

P="${1:-}"
if [ -z "$P" ] || [ "$P" = "-h" ] || [ "$P" = "--help" ]; then
  echo "uso: dc.sh <produto> [--observability] <args do docker compose>" >&2
  exit 1
fi
shift

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROD_DIR="$ROOT/products/$P"

case "$P" in
  ''|.*|*/*|*'\\'*)
    echo "nome de produto inválido: $P" >&2
    exit 1
    ;;
esac

if [ ! -d "$PROD_DIR" ]; then
  echo "produto não encontrado: $PROD_DIR" >&2
  exit 1
fi

ENVF="$PROD_DIR/.env"
EXAMPLEF="$PROD_DIR/.env.example"

PROFILE_ARGS=()
if [ "${1:-}" = "--observability" ]; then
  PROFILE_ARGS+=(--profile observability)
  shift
fi

[ "$#" -gt 0 ] || {
  echo "uso: dc.sh <produto> [--observability] <args do docker compose>" >&2
  exit 1
}

# docker compose exige --env-file existente para interpolação; alguns comandos
# (ex.: config sem variáveis obrigatórias no ambiente) ainda podem falhar se o
# .env estiver incompleto, mas não inventamos valores aqui.
if [ ! -f "$ENVF" ]; then
  if [ -f "$EXAMPLEF" ]; then
    echo "falta $ENVF (copie de $EXAMPLEF ou crie pela TUI)" >&2
  else
    echo "falta $ENVF" >&2
  fi
  exit 1
fi

exec docker compose \
  -p "kodoo-$P" \
  --project-directory "$PROD_DIR" \
  --env-file "$ENVF" \
  -f "$ROOT/common/compose.base.yaml" \
  -f "$ROOT/common/observability.compose.yaml" \
  "${PROFILE_ARGS[@]}" \
  "$@"
