#!/usr/bin/env bash
# Publica una release de McLite SIN posibilidad del bug de la 0.9.0
# (release publicado con el exe reportando una versión vieja).
#
# Uso:  ./scripts/release.sh ["notas del release"]
#
# Garantías:
#   1. La etiqueta SIEMPRE sale de Cargo.toml (única fuente de verdad).
#   2. Aborta si la versión de Cargo.toml NO es mayor que el último release
#      publicado en GitHub (olvidar el bump = el launcher entra en bucle).
#   3. Verifica que el exe recién compilado lleva la versión nueva embebida
#      (comparación diferencial contra el exe del release anterior).
#   4. Verifica el par exe + .sha256 (local y re-descargando el release).
#   5. Deja /home/user/studio/mclite.exe actualizado para descarga manual.

set -euo pipefail
cd "$(dirname "$0")/.."

REPO="SoyJuanPiece/mclite"
TARGET="x86_64-pc-windows-gnu"
OUT="target/$TARGET/release/mclite.exe"
STUDIO_EXE="/home/user/studio/mclite.exe"
NOTES="${1:-}"

# ── 1) Versión local: Cargo.toml manda ──────────────────────────────────────
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "ERROR: versión inválida en Cargo.toml: '$VERSION'" >&2; exit 1;
}
TAG="v$VERSION"
echo "→ Cargo.toml dice $VERSION (etiqueta $TAG)"

# ── 2) Debe ser MAYOR que el último release publicado ───────────────────────
LATEST=$(gh api "repos/$REPO/releases" --jq '.[].tag_name' 2>/dev/null | sed 's/^v//' | sort -V | tail -1 || true)
if [[ -n "${LATEST:-}" ]]; then
  HIGHEST=$(printf '%s\n%s\n' "$LATEST" "$VERSION" | sort -V | tail -1)
  if [[ "$LATEST" == "$VERSION" || "$HIGHEST" != "$VERSION" ]]; then
    echo "ERROR: el último release publicado es $LATEST y Cargo.toml dice $VERSION." >&2
    echo "  Sube la versión en Cargo.toml ANTES de publicar (bug de la 0.9.0)." >&2
    exit 1
  fi
  echo "→ OK: $VERSION > $LATEST (último publicado)"
else
  echo "→ No hay releases previos; primera publicación"
fi

# ── 3) Build Windows ─────────────────────────────────────────────────────────
./scripts/build-windows.sh

# ── 4) El exe lleva la versión nueva embebida (comparación diferencial) ─────
if [[ -n "${LATEST:-}" ]]; then
  TMP=$(mktemp -d)
  gh release download "v$LATEST" -R "$REPO" -p 'mclite.exe' -O "$TMP/prev.exe" 2>/dev/null
  PREV=$(grep -aoF "$VERSION" "$TMP/prev.exe" 2>/dev/null | wc -l | tr -d ' ')
  NOW=$(grep -aoF "$VERSION" "$OUT" | wc -l | tr -d ' ')
  rm -rf "$TMP"
  if [[ "$NOW" -le "$PREV" ]]; then
    echo "ERROR: el exe compilado NO lleva la versión $VERSION embebida." >&2
    echo "  ¿Seguro que Cargo.toml se usó para este build? Abortando." >&2
    exit 1
  fi
  echo "→ OK: exe lleva \"$VERSION\" embebida ($NOW ocurrencias vs $PREV del anterior)"
fi

# ── 5) Integridad local del par exe + sha256 ────────────────────────────────
( cd "$(dirname "$OUT")" && sha256sum -c mclite.exe.sha256 --quiet ) \
  || { echo "ERROR: el .sha256 local no coincide con el exe" >&2; exit 1; }
echo "→ OK: sha256 local verificado"

# ── 6) Commit de Cargo.toml/Cargo.lock si están sucios ──────────────────────
if ! git diff --quiet -- Cargo.toml Cargo.lock; then
  git add Cargo.toml Cargo.lock
  git -c user.name="SoyJuanPiece" -c user.email="177440697+SoyJuanPiece@users.noreply.github.com" \
    commit -m "v$VERSION: bump de versión para el release $TAG"
  echo "→ Commit de versión creado"
fi
git push origin main 2>&1 | tail -1

# ── 7) Publicar ──────────────────────────────────────────────────────────────
cp "$OUT" /tmp/mclite.exe
cp "$OUT.sha256" /tmp/mclite.exe.sha256
gh release create "$TAG" \
  /tmp/mclite.exe#/mclite.exe /tmp/mclite.exe.sha256#/mclite.exe.sha256 \
  -R "$REPO" --title "$TAG" --notes "${NOTES:-McLite $TAG}"
echo "→ Release $TAG publicado"

# ── 8) Verificación final: re-descargar y validar, y actualizar el artefacto ─
sleep 3
VDIR=$(mktemp -d)
gh release download "$TAG" -R "$REPO" -p 'mclite.exe' -p 'mclite.exe.sha256' -D "$VDIR" \
  || { echo "ERROR: no pude re-descargar el release recién creado" >&2; exit 1; }
( cd "$VDIR" && sha256sum -c mclite.exe.sha256 --quiet ) \
  || { echo "ERROR: los assets publicados NO pasan la verificación" >&2; exit 1; }
PUB=$(grep -aoF "$VERSION" "$VDIR/mclite.exe" | wc -l)
[[ "$PUB" -gt 0 ]] || { echo "ERROR: el exe publicado no lleva $VERSION" >&2; exit 1; }
cp "$VDIR/mclite.exe" "$STUDIO_EXE"
rm -rf "$VDIR"
echo
echo "TODO OK: $TAG publicado, verificado y $STUDIO_EXE actualizado."
