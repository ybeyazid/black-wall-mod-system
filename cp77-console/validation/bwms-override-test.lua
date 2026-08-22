-- bwms-override-test.lua — PROVA determinística do Override-suppress (retorno POD).
--
-- Requer: dylib --features lua (libcp77_console-LUA-TESTE.dylib) + a sonda
-- bwms-override-probe.reds compilada + um V VIVO (carregue um save antes).
-- Rodar no console (tecla `): loadmod <caminho>/bwms-override-test.lua
--
-- Como prova: ObserveAfter só dispara DEPOIS da original; no suppress a original é
-- PULADA, então o ObserveAfter NÃO dispara e o contador da sonda NÃO sobe. Se mesmo
-- assim o retorno vier 42 (e não 7), provou: retorno reescrito + original suprimida.

local p = Game.GetPlayer()
if not p then
  print("[ov-test] sem player — carregue um save e rode de novo")
  return
end

local before = p:BwmsProbeCalls()
print("[ov-test] contador inicial = " .. tostring(before))

-- sinaliza se a ORIGINAL rodou (ObserveAfter NÃO dispara quando há suppress)
local ran_original = false
ObserveAfter("PlayerPuppet", "BwmsProbe", function(self) ran_original = true end)

-- override TOTAL: devolve 42 e NÃO chama wrapped() → deve SUPRIMIR a original
Override("PlayerPuppet", "BwmsProbe", function(self) return 42 end)

-- espera ~1.5s pro registro DRENAR (drain roda no tick da thread do jogo), depois testa
Cron.After(1.5, function()
  local r = p:BwmsProbe()
  local after = p:BwmsProbeCalls()
  local marshaling = (r == 42)
  local suppressed = (after == before) and (ran_original == false)
  print(string.format(
    "[ov-test] retorno=%s (esperado 42) | contador %d->%d | original_rodou=%s",
    tostring(r), before, after, tostring(ran_original)))
  if marshaling and suppressed then
    print("[ov-test] >>> OVERRIDE-SUPPRESS POD: OK <<<  (retorno reescrito + original suprimida)")
  elseif marshaling and not suppressed then
    print("[ov-test] PARCIAL: retorno 42 OK, mas a ORIGINAL rodou (suppress falhou; caiu no rewrite)")
  else
    print("[ov-test] FALHOU: retorno=" .. tostring(r) .. " (override nao disparou? registro nao drenou? rode de novo)")
  end
end)
