// codeware-visualcontrollercomponent.reds — Codeware `#33` (`entVisualControllerComponent.reds`,
// catálogo exaustivo Codeware). CORREÇÃO DE RISCO 2026-08-18 (offline, mesma técnica que
// destravou o item `#107` nesta sessão): a nota de risco de 2026-08-12 pra este item ("declarar a
// classe arriscaria o crash de bind RTTI, tipo possivelmente ausente da RTTI") checou só
// `redscript-src` (grep, zero hits pro nome da classe) e nunca cruzou contra o header GERADO da
// reflection real do jogo (`RED4ext.SDK/.../Generated/ent/VisualControllerComponent.hpp`, "This
// file is generated from the Game's Reflection data" — confiança máxima). Esse header confirma os
// 4 campos citados na nota original com offsets exatos e auto-consistentes (cross-referenciados
// contra 3 outros headers vendorizados independentes — ver comentário grande em
// `register_vcc_33_natives`, cp77-console/src/register.rs).
//
// A classe `entVisualControllerComponent` em si CONTINUA não-declarada (mesmo risco do `#30`/
// CMesh-trap pra declarar `native class`/ancestralidade nova) — mas os CAMPOS não precisam disso:
// mesma técnica já usada pelo item irmão `#213` (`codeware-visualcontroller.reds`, acima na mesma
// sessão de catálogo) e por `#71`/`#168`. O parâmetro é sempre `ref<IComponent>` (tipo vanilla
// 100% seguro), obtido via `Entity.FindComponentByType(n"entVisualControllerComponent")` (item
// `#24`, PROVADO AO VIVO desde 2026-08-10 — usa só o CName pra busca, nunca declara a classe).
//
// Campos expostos (leitura pura, zero mutação, zero RawFunc/vtable):
//   - meshProxy: Ref<CMesh>@0x90 → hash do ResourcePath embutido (BwmsVccMeshProxyHash).
//   - appearanceDependency: DynArray<VisualControllerDependency>@0xA8 (stride 0x18/entry:
//     mesh:RaRef<CMesh>@+0x00/appearanceName:CName@+0x08/componentName:CName@+0x10) → count +
//     3 getters indexados (BwmsVccAppearanceDependencyCount/MeshHash/AppearanceName/ComponentName).
//   - cookedAppearanceData: RaRef<CookedAppearanceData>@0xB8 → hash do ResourcePath embutido
//     (BwmsVccCookedAppearanceDataHash).
//   - forcedLodDistance: enum C++ de 1 byte (valores 0-8)@0xF0 → Int32 cru
//     (BwmsVccForcedLodDistance; sentinela -1 = componente inválido/ilegível).
//
// Fora de escopo, de propósito: LoadAppearanceDependencies() (o único MÉTODO real do item) —
// precisaria de endereço C++ (RE não feita) ou de declarar a classe (risco não resolvido pra
// MÉTODOS). Item `#33` avança de "0-código" pra "7 campos/getters lidos, código pronto" — prova
// ao vivo pendente.
native func BwmsVccMeshProxyHash(component: ref<IComponent>) -> Uint64
native func BwmsVccAppearanceDependencyCount(component: ref<IComponent>) -> Int32
native func BwmsVccAppearanceDependencyMeshHash(component: ref<IComponent>, idx: Int32) -> Uint64
native func BwmsVccAppearanceDependencyAppearanceName(component: ref<IComponent>, idx: Int32) -> CName
native func BwmsVccAppearanceDependencyComponentName(component: ref<IComponent>, idx: Int32) -> CName
native func BwmsVccCookedAppearanceDataHash(component: ref<IComponent>) -> Uint64
native func BwmsVccForcedLodDistance(component: ref<IComponent>) -> Int32
