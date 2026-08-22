# bwms-proceed — auto-skip da tela "APERTE [espaço] PARA CONTINUAR"

Helper que pula a **engagement screen** (o "APERTE [espaço] PARA CONTINUAR", com vídeo de fundo) do boot
do Cyberpunk 2077 no macOS, indo direto pro main menu — sem você apertar nada.

## Por que é um helper separado (e não parte da dylib)

O proceed dessa tela é **input HID nativo**. Descobrimos (23 ciclos de RE) que:
- Injetar o "E" de **dentro** do processo do jogo é **ignorado pelo macOS** (proteção anti-loop).
- Forçar a "phase byte" do dispatcher de boot **crasha** (assert) e não fecha a tela (é uma camada paralela).
- O redscript **não alcança** o proceed (a tela é view puro; o evento de fechar é não-construível).

O único caminho que funciona é um **processo EXTERNO** com Acessibilidade injetando SPACE — que é
exatamente o que o teclado real (e o computer-use) faz. Este binário é esse processo.

## Setup (uma vez)

1. **Acessibilidade:** Ajustes → Privacidade e Segurança → **Acessibilidade** → **+** → adicione
   `bwms-proceed` (este binário). Deixe o interruptor **azul/ligado**.
   (É por isso que a Acessibilidade que você deu ao *jogo* não bastou — quem injeta é ESTE binário.)

## Uso

Rode junto do jogo (ele espera o Cyberpunk subir sozinho):

```
nohup /caminho/para/bwms-proceed >/tmp/bwms-proceed.log 2>&1 &
open steam://run/1091500
```

- Mantenha a **janela do jogo em foco** durante o boot (a injeção HID vai pro app frontmost).
- O helper detecta a engagement pelo log do BWMS (`/tmp/cp77-console.log`, linha
  `engagement ativa = true`, marcada pelo redscript) e injeta SPACE até entrar no menu.
- Fallback por tempo se o log não marcar. Sai sozinho quando o jogo fecha ou entra no menu.

## Como funciona (resumo técnico)

`CGEventPost(kCGHIDEventTap, keyDown/Up SPACE)` (keyCode 49) — injeta no HID stream, o nível que o
CP2077 lê, idêntico ao teclado. Fallback `CGEventPostToPid` pro PID do jogo. Detalhe completo da RE
em `cp77-symbols/notes/boot-flow-phase-byte.md` (seções "PLANO B" e "RESULTADO dos ciclos").
