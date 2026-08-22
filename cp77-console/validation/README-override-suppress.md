# Validação in-game do Override-suppress (retorno-com-valor POD)
> **Paths:** `$GAME` = a instalação do Cyberpunk 2077; `$BWMS_REPO` = a raiz deste
> repositório. Exporte os dois antes de rodar os comandos abaixo (ou use `BWMS_GAME`).


Prova **determinística** num boot. A sonda é um método que o jogo nunca chama —
sobrescrevê-lo não afeta nada (risco zero). Tudo reversível.

`GAME` = `$GAME`

---

## 1) Instalar a sonda redscript + compilar

Copiar a sonda pra pasta dos mods redscript:

```
cp "$BWMS_REPO/cp77-console/validation/bwms-override-probe.reds" "$GAME/r6/scripts/blackwall-mods/"
```

Compilar (reescreve o final.redscripts; backup .bwms-bak já existe):

```
"$BWMS_REPO/redscript/target/release/scc" -compile "$GAME/r6/scripts"
```

## 2) Trocar a dylib pela build de TESTE (--features lua), com backup da produção

```
cp "$GAME/red4ext/libcp77_console.dylib" "$GAME/red4ext/libcp77_console.dylib.PROD-BAK"
cp "$BWMS_REPO/dist/test-builds/libcp77_console-LUA-TESTE.dylib" "$GAME/red4ext/libcp77_console.dylib"
```

## 3) Armar o auto-teste (1 comando aqui) — NO JOGO não digita NADA

```
touch /tmp/bwms-ovtest
```

Depois: abrir o jogo → apertar a tecla `` ` `` (abre o console, isso dispara o auto-teste) →
CARREGAR UM SAVE (precisa de V vivo). O teste roda **sozinho** e em ~2s loga no console:

```
[ov-test] >>> OVERRIDE-SUPPRESS POD: OK <<<  (retorno reescrito + original suprimida)
```

(Se preferir o jeito manual, o `loadmod .../bwms-override-test.lua` ainda funciona — mas o
auto-teste dispensa digitar comando.)

- **OK** = o retorno virou 42 (marshaling do POD) **e** a original foi pulada (contador parado, ObserveAfter não disparou). Override-suppress de retorno-com-valor **provado**.
- **PARCIAL** (retorno 42 mas original rodou) = caiu no rewrite, suppress não pegou → me avisa.
- **FALHOU** = override não disparou (registro não drenou?) → rode o `loadmod` de novo (dá mais tempo de drenar).

## 4) Reverter pra produção (0% Lua) quando terminar

```
cp "$GAME/red4ext/libcp77_console.dylib.PROD-BAK" "$GAME/red4ext/libcp77_console.dylib"
rm "$GAME/r6/scripts/blackwall-mods/bwms-override-probe.reds"
rm /tmp/bwms-ovtest
```

E recompilar (passo 1, comando de compilar) pra tirar a sonda do final.redscripts.

---

## O que isto prova (e o que NÃO precisa provar)

- **Prova:** override de método que RETORNA VALOR (POD: Bool/Int/Float/...) agora SUPRIME a
  original (não só reescreve o retorno) — a peça que faltava.
- **Já provado antes (memória):** suppress de método VOID (OnDamageReceived pulou a original) +
  wrapped Override (chama a original na ordem). O executor-replace (`Interceptor::replace`) é
  estável.
- **Fora de escopo (limite inerente):** override-TOTAL de retorno NÃO-POD (classe/handle/string)
  não suprime — cai no rewrite (roda original + sobrescreve). Não dá pra fabricar objeto complexo
  seguro; o rewrite cobre o uso real (getter).
- **Produção (0% Lua) não usa nada disto:** Override vive na trilha `--features lua` (compat CET).
  Os 6 enablers shipados usam redscript (compile-time) + Codeware (nativo) + os mecanismos já
  provados. Override-suppress completa a CET; não é gargalo dos 6 no pacote público.
