// bwms-override-probe.reds — SONDA de validação do Override-suppress (descartável).
//
// Define um método que o JOGO NUNCA chama (BwmsProbe). Existe só pro teste:
// sobrescrevê-lo NÃO afeta nada do jogo (risco zero). O campo é runtime-only
// (NÃO vai pro save). Depois de validar, pode apagar este .reds e recompilar.
//
// Lógica do teste: a "original" tem um EFEITO COLATERAL observável (incrementa um
// contador) e retorna 7. Um Override-TOTAL que devolve 42 SEM chamar wrapped() deve
// SUPRIMIR a original → o contador NÃO sobe e a chamada devolve 42 (não 7).

@addField(PlayerPuppet)
let m_bwmsProbeCalls: Int32;

// "Original": efeito colateral (contador++) + retorno conhecido (7).
@addMethod(PlayerPuppet)
public func BwmsProbe() -> Int32 {
  this.m_bwmsProbeCalls += 1;
  return 7;
}

// Getter do contador — NÃO é sobrescrito; o teste lê pra saber se a original rodou.
@addMethod(PlayerPuppet)
public func BwmsProbeCalls() -> Int32 {
  return this.m_bwmsProbeCalls;
}
