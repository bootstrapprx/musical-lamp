#!/usr/bin/env bash
# Uso: common/build-image.sh <trivia|accounting> <release>   (PUSH=1 para publicar)
set -euo pipefail

P="${1:?uso: build-image.sh <trivia|accounting> <release>}"
REL="${2:?informe a release (ex.: 2026.11.1)}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PD="$ROOT/products/$P"
[ -d "$PD" ] || {
  echo "produto desconhecido: $P" >&2
  exit 1
}

set -a
. "$ROOT/common/versions.env"
. "$PD/build.env"
set +a

: "${FRAPPE_REF:?preencha FRAPPE_REF em $PD/build.env (tag ou commit)}"
case "$FRAPPE_REF" in
version-* | develop | main | master)
  echo "FRAPPE_REF nao pode ser branch movel: $FRAPPE_REF" >&2
  exit 1
  ;;
esac
if grep -q '[<>]' "$PD/apps.json"; then
  echo "apps.json ainda tem placeholders (<...>): $PD/apps.json" >&2
  exit 1
fi
[ -n "${BENCH_VERSION:-}" ] || echo "AVISO: BENCH_VERSION vazio (build nao reprodutivel)" >&2

CACHE_BUST="$(sha256sum "$PD/apps.json" | cut -d' ' -f1)"
REVISION="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)"
OUT=(--load)
[ "${PUSH:-0}" = "1" ] && OUT=(--push)

docker buildx build \
  --file "$ROOT/Dockerfile" \
  --secret "id=apps_json,src=$PD/apps.json" \
  --build-arg "PYTHON_VERSION=$PYTHON_VERSION" \
  --build-arg "DEBIAN_BASE=$DEBIAN_BASE" \
  --build-arg "NODE_VERSION=$NODE_VERSION" \
  --build-arg "NVM_VERSION=$NVM_VERSION" \
  --build-arg "BENCH_VERSION=${BENCH_VERSION:-}" \
  --build-arg "FRAPPE_BRANCH=$FRAPPE_REF" \
  --build-arg "CACHE_BUST=$CACHE_BUST" \
  --build-arg "INSTALL_WKHTMLTOPDF=$INSTALL_WKHTMLTOPDF" \
  --build-arg "INSTALL_CHROMIUM=$INSTALL_CHROMIUM" \
  --build-arg "IMAGE_TITLE=$IMAGE_TITLE" \
  --build-arg "IMAGE_VERSION=$REL" \
  --build-arg "IMAGE_REVISION=$REVISION" \
  --tag "$IMAGE:$REL" \
  "${OUT[@]}" \
  "$ROOT"

echo
echo "Imagem: $IMAGE:$REL"
if [ "${PUSH:-0}" = "1" ]; then
  docker buildx imagetools inspect "$IMAGE:$REL" | sed -n '1,4p'
  echo "Promova para producao pelo DIGEST (IMAGE_REF=$IMAGE@sha256:...)."
else
  echo "Manifesto de apps (commits resolvidos):"
  docker run --rm --entrypoint cat "$IMAGE:$REL" /home/frappe/frappe-bench/BUILD_MANIFEST
fi
