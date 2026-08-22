// codeware-inksystem-clipboard.reds — Codeware `App/UI/inkSystem.hpp::GetClipboardText`/
// `SetClipboardText` (PENDENCIAS-UNIFICADAS.md item #100, candidato barato achado 2026-08-11,
// 4ª rodada de mineração da sessão).
//
// A fonte real (`inkSystem extends IGameSystem`) tem 5 métodos: `GetLayers`/`GetLayer`/
// `GetWorldWidgets` (precisam do singleton C++ `Red::InkSystem::Get()`, endereço
// `Red::AddressLib::InkSystem_Instance` nunca resolvido no Mac — FORA de escopo aqui) e
// `GetClipboardText`/`SetClipboardText` (usam a lib vendorizada de 3o `clip::get_text`/
// `clip::set_text`, ZERO dependência da engine do jogo — só o sistema operacional).
//
// DIVERGÊNCIA DE ESCOPO CONSCIENTE: em vez de forjar a classe `inkSystem` inteira (RTTI class
// nova, mesma categoria de risco de CallbackSystem/ScriptableService, JUSTIFICÁVEL só se os
// outros 3 métodos também fossem implementados) só pra hospedar 2 métodos sem estado de
// instância nenhum, exponho como globais `BwmsGetClipboardText`/`BwmsSetClipboardText` — a MESMA
// capacidade prática (ler/escrever a área de transferência do sistema), zero forge de classe.
// Implementação Rust reusa a ponte NSPasteboard já PROVADA EM PRODUÇÃO (editor de texto do
// console ImGui, Cmd+C/Cmd+X/Cmd+V há sessões) — `overlay.rs::clipboard_get`/`clipboard_set`,
// extraídas pra funções standalone reusáveis, zero código de baixo nível novo.
//
// Limite documentado (mesma categoria já aceita no projeto, ex. `BwmsConfigGet`): a LEITURA
// (`GetClipboardText`) só suporta strings <=19 bytes (limite SSO do `write_cstring_inline_ret` —
// string mais longa que isso volta vazia). Suficiente pro caso comum (copiar um comando/código
// curto). A ESCRITA (`SetClipboardText`) não tem esse limite (lê o argumento via
// `read_params_consuming_with_strings`, que já suporta string arbitrária).
native func BwmsGetClipboardText() -> String
native func BwmsSetClipboardText(text: String) -> Bool
