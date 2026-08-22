// codeware-visualcontroller.reds — Codeware `#213` (`Red/VisualController.hpp`, catálogo
// exaustivo Codeware, round 12 do sweep, 2026-08-12). Fonte real é um struct C++ INTERNO puro
// (`Red::VisualController` / `Raw::VisualControllerComponent::Controller@0xD8`) — não é RTTI
// redscript-facing e não tem `RawFunc` além de `LoadDependencies` (fora de escopo aqui, RawFunc
// sem endereço Mac). Groundwork já existia desde a mineração de `#33`/`#167` (round 9, o probe
// observe-only `dump_visualcontroller_via_receiver` em `register.rs` lê exatamente este layout
// via o `receiver` do thunk de evento) — nunca tinha virado uma capacidade PRÓPRIA/reusável,
// independente do evento `AppearanceEvent`/`AppearanceLODsDistanceOverrideEvent` disparar.
//
// Esta declaração expõe 4 leituras de campo PURAS (zero mutação, zero RawFunc, zero vtable) via
// composição com `Entity.FindComponentByType` (item `#24`, PROVADO AO VIVO desde 2026-08-10) —
// zero endereço/RE nova: `entity.FindComponentByType(n"entVisualControllerComponent")` devolve
// (se existir) o componente real como `ref<IComponent>` (tipo VANILLA seguro), que se passa
// direto pras 4 natives abaixo.
//
// ⚠️ RISCO DE TIPO: `entVisualControllerComponent` é `native class extends IComponent` com ZERO
// ocorrências em `redscript-src` inteiro (mesma categoria CMesh-trap já corretamente flagada pro
// item irmão `#33` nesta sessão) — por isso NUNCA declarada aqui (nem em nenhum outro arquivo
// do projeto). Só o NOME (CName, hash de compile-time) é usado pra busca — não precisa da classe
// declarada pra isso funcionar. As natives abaixo aceitam `ref<IComponent>` genérico; se o
// componente passado não for de fato um `VisualControllerComponent`, o offset `+0xD8` lê dado
// implausível — sempre atrás de `gum::is_readable` no lado Rust, então NUNCA crasha, só devolve
// valor sem sentido nesse caso (mesma divergência já aceita pro `receiver` aproximado do `#167`).
//
// ⚠️ REGRA DE OURO: NUNCA testado ao vivo nesta rodada (100% offline) — offsets confirmados só
// pelo header vendorizado real (`RED4EXT_ASSERT_SIZE(VisualController,0x80)`/`RED4EXT_ASSERT_
// OFFSET(dependencies,0x10)`/`RED4EXT_ASSERT_OFFSET(lock,0x7C)`), nunca contra memória viva desta
// build Mac. Item `#213` continua REAL_GAP até um boot confirmar.
native func BwmsVisualControllerHasController(component: ref<IComponent>) -> Bool
native func BwmsVisualControllerStatus(component: ref<IComponent>) -> Int32
native func BwmsVisualControllerDependencyCount(component: ref<IComponent>) -> Int32
native func BwmsVisualControllerLooseDependencyCount(component: ref<IComponent>) -> Int32
