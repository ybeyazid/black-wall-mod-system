#!/bin/bash
# deploy.sh — build + DEPLOY SEGURO do dylib do runtime BWMS.
# LIÇÃO (2026-06-26): copiar o dylib cru SEM assinar → crash CODESIGNING (Invalid Page) no
# load → jogo morre. macOS exige ad-hoc sign + xattr limpo. ESTE script faz tudo certo e
# VERIFICA com codesign -v antes de manter (senão restaura o backup). Use SEMPRE este, nunca `cp`.
set -euo pipefail
cd "$(dirname "$0")"

# Path do jogo: BWMS_GAME (env) > paths conhecidos. Sobrevive a mudança de M.2/volume — depois de mover:
#   BWMS_GAME='/novo/path/Cyberpunk 2077' ./deploy.sh   (ou, se for o path padrão da Steam, acha sozinho)
GAME_DIR="${BWMS_GAME:-}"
if [ -z "$GAME_DIR" ]; then
  for c in \
    "${CP77_DIR:-}" \
    "$HOME/Library/Application Support/Steam/steamapps/common/Cyberpunk 2077" \
    "/Applications/Cyberpunk 2077.app"; do
    [ -z "$c" ] && continue
    [ -f "$c/Cyberpunk2077.app/Contents/MacOS/Cyberpunk2077" ] && GAME_DIR="$c" && break
  done
fi
# Biblioteca Steam em disco EXTERNO — caso comum, e nenhum caminho fixo acerta. Antes isto era
# resolvido embutindo o nome do disco do autor na lista acima, o que só funcionava pra uma pessoa
# e ainda vazava o nome do volume dela no repositório publicado.
if [ -z "$GAME_DIR" ] && [ -d /Volumes ]; then
  for v in /Volumes/*; do
    for suf in "SteamLibrary/steamapps/common/Cyberpunk 2077" \
               "steamapps/common/Cyberpunk 2077" \
               "Games/Steam/steamapps/common/Cyberpunk 2077"; do
      [ -f "$v/$suf/Cyberpunk2077.app/Contents/MacOS/Cyberpunk2077" ] && { GAME_DIR="$v/$suf"; break 2; }
    done
  done
fi
[ -n "$GAME_DIR" ] && [ -d "$GAME_DIR/red4ext" ] || { echo "ERRO: jogo nao achado. Rode:  BWMS_GAME='/caminho/Cyberpunk 2077' ./deploy.sh"; exit 1; }
echo "=== jogo: $GAME_DIR ==="
DEPLOY="$GAME_DIR/red4ext/libcp77_console.dylib"
NEW="target/release/libcp77_console.dylib"

echo "=== 1) build DEV (build-core.sh + --features devtools: auto-proceed + logs) ==="
# deploy.sh = build de DEV/teste → liga o guarda-chuva `devtools` (autoproceed=injeção de SPACE p/
# pular a engagement + devlog=logs verbosos). O build PÚBLICO (do zip) é `./build-core.sh` SEM features
# (sem CGEvent = sem padrão keylogger no binário; ver dist/pack-bwms.sh + DEFLAG-E-INSTALL-PLANO.md).
./build-core.sh --features devtools 2>&1 | { grep -iE "error\[|Finished|OK: core|loadable" || true; }
[ -f "$NEW" ] || { echo "ERRO: build não gerou $NEW"; exit 1; }

echo "=== 2) backup do deployado atual ==="
BAK="$DEPLOY.bak-$(date +%Y%m%d-%H%M%S)"
cp "$DEPLOY" "$BAK" && echo "  backup: $(basename "$BAK")"

echo "=== 3) deploy + assina ad-hoc + xattr ==="
cp "$NEW" "$DEPLOY"
codesign --remove-signature "$DEPLOY" 2>/dev/null || true
codesign --force --sign - "$DEPLOY"
xattr -cr "$DEPLOY"

echo "=== 4) VERIFICA (codesign -v) — se falhar, restaura o backup ==="
if codesign -v "$DEPLOY" 2>&1; then
  echo "✅ codesign -v OK — deploy assinado e válido. Carrega no próximo boot."
  ls -la "$DEPLOY" | awk '{print "  ",$6,$7,$8,$5"B"}'
else
  echo "❌ codesign -v FALHOU — restaurando backup (jogo continua funcional)"
  cp "$BAK" "$DEPLOY"
  exit 1
fi
