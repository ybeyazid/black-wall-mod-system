#!/usr/bin/env bash
# PLAY.command — lança o auto-skip (bwms-proceed) + o Cyberpunk. 2 cliques.
# PRÉ-REQUISITO (1x): dar Acessibilidade ao binário bwms-proceed (Ajustes > Privacidade e
# Segurança > Acessibilidade). Sem isso o macOS bloqueia a injeção do SPACE.
HELPER="$(cd "$(dirname "$0")" && pwd)/target/release/bwms-proceed"
[ -x "$HELPER" ] || { echo "bwms-proceed não compilado (cargo build --release na pasta bwms-proceed)"; exit 1; }
pkill -f bwms-proceed 2>/dev/null
nohup "$HELPER" >/tmp/bwms-proceed.log 2>&1 &
echo "auto-skip rodando (log: /tmp/bwms-proceed.log). Abrindo o jogo — mantenha a janela em foco no boot."
open "steam://run/1091500"
