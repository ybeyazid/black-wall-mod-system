// -----------------------------------------------------------------------------
// Codeware — Entity.AddComponent (cw-entity-builder, 2026-07-24)
// -----------------------------------------------------------------------------
//
// ✅ BISECÇÃO 2026-07-25 CONCLUÍDA (boot ao vivo): este arquivo + codeware-ui-customcontroller.reds
// JUNTOS (sem codeware-reflection.reds) bootam LIMPO — GAMEPLAY real confirmada, autocontinue
// completo, zero crash. O culpado isolado é EXCLUSIVAMENTE codeware-reflection.reds (crash
// reproduzido sozinho, mesmo endereço 0x103da2a60, ver nota lá). Este arquivo está LIBERADO pro
// bundle shipado/deployado.
//
// Atalho pragmático (achado pela investigação paralela do mesmo nome): em vez de forjar
// `Red::EntityBuilder`/`EntityBuilderWrapper`/`EntityBuilderTemplateWrapper` (3 classes novas +
// RE de endereço nativo desconhecido pro pipeline de spawn/extração de appearance — XL genuíno,
// mesma categoria já esgotada em `axl-factories-apply`/`axl-resource-patch-apply`), reproduz o que
// o PRÓPRIO Codeware faz pro caso comum "adicionar um componente" — `App/Entity/EntityEx.hpp`:
//
//   struct EntityEx : Red::Entity {
//       void AddComponent(const Red::Handle<Red::IComponent>& aComponent) {
//           components.PushBack(aComponent);
//       }
//   };
//   RTTI_EXPAND_CLASS(Red::Entity, App::EntityEx, { RTTI_METHOD(AddComponent); ... });
//
// — o Codeware real TAMBÉM estende a classe `Entity` já existente aqui, NÃO usa `EntityBuilder`
// (esse só entra pro caso mais raro/fundo de mexer em "Entity/Extract", antes do Entity existir).
//
// `@addMethod(Entity)` é o mecanismo do PRÓPRIO COMPILADOR redscript (zero RTTI-forge), já usado
// dezenas de vezes neste projeto em classes vanilla reais (PlayerPuppet/
// SettingsSelectorControllerBool/SingleplayerMenuGameController/EngagementScreenGameController/
// PauseMenuGameController/...) — decisão deliberada em vez de `register_method` no Rust
// (register.rs::register_entity_builder_natives): `register_method` só tem precedente comprovado
// em classes FORJADAS por nós (CallbackSystem/CallbackSystemHandler/EntityLifecycleEvent/...),
// nunca numa classe REAL VANILLA já populada pelo motor como `Entity` — risco desconhecido que não
// precisa ser corrido, já que `@addMethod` (comprovadamente seguro em classes vanilla) resolve
// igual. O trabalho de verdade (push no `DynArray<Handle<IComponent>>` real do motor, em
// `Entity+0xA0`) mora na global nativa `BwmsEntityAddComponent` (register.rs) — mesma receita
// 100% provada de `BwmsCallMethod` (register_global_composed + compose_params_from_types).
//
// NÃO TESTADO EM BOOT AINDA — ver a hipótese fundamentada (não confirmada) em
// register.rs::tramp_entity_add_component sobre por que isto deveria bastar sem lógica de
// anexação adicional (o disparo de "Entity/Assemble" em lib.rs roda ANTES do `Attach` nativo).

native func BwmsEntityAddComponent(entity: ref<Entity>, component: ref<IComponent>) -> Bool;

@addMethod(Entity)
public func AddComponent(component: ref<IComponent>) -> Void {
    BwmsEntityAddComponent(this, component);
}

// 2026-08-10 (cont.187): `FindComponentByType` — 2º dos 6 métodos do item #24, leitura da MESMA
// DynArray que `AddComponent` escreve. Zero RE nova (mesmo offset `Entity+0xA0`, mesmo
// `class_of`/`type_name_getname` já provados). Assinatura real do Codeware.
native func BwmsEntityFindComponentByType(entity: ref<Entity>, type: CName) -> ref<IComponent>;

@addMethod(Entity)
public func FindComponentByType(type: CName) -> ref<IComponent> {
    return BwmsEntityFindComponentByType(this, type);
}

// 2026-08-10 (cont.190): `GetComponents` — 3º dos 6 métodos do item #24. 2º array-retorno-de-
// native deste projeto (o 1º, TweakXL #43/GetRecords, achou+corrigiu o crash de teardown do
// array local via trailer de allocator-vft emprestado — ver cont.188/189). Aqui uso o trailer do
// PRÓPRIO `Entity+0xA0` que estou lendo, sem precisar de doador externo.
native func BwmsEntityGetComponents(entity: ref<Entity>) -> array<ref<IComponent>>;

@addMethod(Entity)
public func GetComponents() -> array<ref<IComponent>> {
    return BwmsEntityGetComponents(this);
}

// 2026-08-10 (cont.191): `GetTemplatePath` — 4º dos 6 métodos do item #24. Achado que destravou:
// header vendorizado `entEntity.hpp` (diferente do `Generated/ent/Entity.hpp`, opaco) documenta
// `templatePath: ResourcePath @ Entity+0x60` — `ResourcePath` é só um hash de 8 bytes (mesma
// convenção de `resource_path_hash`), leitura direta de campo, zero RE nova.
native func BwmsEntityGetTemplatePath(entity: ref<Entity>) -> ResRef;

@addMethod(Entity)
public func GetTemplatePath() -> ResRef {
    return BwmsEntityGetTemplatePath(this);
}
