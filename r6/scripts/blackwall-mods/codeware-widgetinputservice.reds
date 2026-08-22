// codeware-widgetinputservice.reds — Codeware `App/UI/WidgetInputService.hpp` (item #176,
// catálogo exaustivo Codeware, round 10, 2026-08-12). Fonte real (`WidgetInputService.cpp`):
// `IsCharacterInput(EInputKey)` é 100% comparação de ORDINAL (zero chamada C++/RTTI) e
// `ToCharacter(EInputKey)` só chama Win32 (`GetKeyboardState`+`ToUnicode`) DEPOIS de já ter
// passado por `IsCharacterInput` — as DUAS peças aqui portadas são a metade "lógica pura" do
// item, zero endereço nativo.
//
// A peça que GENUINAMENTE precisa de RE (fora de escopo desta rodada, XL): o HOOK
// (`HookBefore<Raw::inkSystem::ProcessCharacterEvent>`, dispatch REAL de evento pra um widget
// focado) — endereço Mac nunca resolvido, mesma categoria XL de `#100`/`Red::InkSystem::Get()`
// (busca de símbolo/string negativa, confirmado nesta mesma sessão). Sem o hook, estas 2
// funções são capacidade PRÓPRIA do BWMS (utilitário de conversão tecla→caractere,
// reusável por qualquer mod que precise disso — ex. campo de texto customizado — mesmo sem o
// dispatch automático de "OnInputKey" que o Codeware real faria).
//
// Achado-chave REUSADO (já confirmado nesta base de código, `cw-rawinput-realname`,
// 2026-07-18): pra letras/dígitos/espaço, `EInputKey` JÁ É o código ASCII do caractere —
// zero tabela necessária pra esses. Pontuação/numpad usam tabela ESTÁTICA US-QWERTY —
// DIVERGÊNCIA HONESTA vs. o `ToUnicode` real do Windows (que respeita layout/locale do
// usuário; aqui é sempre US-QWERTY, sem Caps Lock persistente — `shift` é parâmetro
// explícito, não lido de um `GetKeyboardState` real). Ver nota grande em
// `cp77-console/src/register.rs::is_character_input_key`/`to_character_key`.
//
// ⚠️ REGRA DE OURO: pura lógica, zero endereço/offset de memória — mas nunca testado ao
// vivo (sem processo do jogo pra chamar via canal nesta rodada offline). Item `#176` continua
// REAL_GAP (a peça de dispatch real, `OnCharacterInput`/hook, segue ausente).
native func BwmsIsCharacterInput(key: EInputKey) -> Bool
native func BwmsToCharacterKey(key: EInputKey, shift: Bool) -> String
