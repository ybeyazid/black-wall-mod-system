// codeware-iserializable-postload.reds — Codeware `Scripting/ISerializable.reds::ProcessPostLoad`/
// `RefreshResource` (item #58, catálogo exaustivo, round 9 — 2026-08-12, agente offline
// dedicado). Fonte real: `public abstract native class ISerializable { ... native func
// ProcessPostLoad(opt disablePreInitialization: Bool) ... }` — mas `ISerializable` tem ZERO
// ocorrências em `redscript-src` (grep confirmado no corpus inteiro), mesma categoria
// CMesh-trap já documentada à exaustão neste catálogo (#20/#21/#29/#30/#33/#43/#83/#86/#91/#145):
// declarar/`@addMethod` sobre um tipo `native class` sem backing script-facing arrisca o mesmo
// crash de bind RTTI já provado no `#30`.
//
// **Divergência de escopo deliberada, dentro da filosofia já estabelecida (CLAUDE.md "REGRA DE
// NOME/API" — framework de 3os, sem obrigação de bater 1:1)**: expus em `IScriptable` em vez de
// `ISerializable`. `IScriptable` É um tipo vanilla real e seguro (`redscript-src/orphans.script:
// 11256`, `public native class IScriptable`, já usado extensivamente pelo projeto inteiro) e,
// na hierarquia C++ real, herda a MESMA posição de slot de vtable pra `PostLoad` (slots da
// classe-base nunca se movem por herança em C++). O offset do slot (Mac +0x30, Windows +0x28 do
// header `RED4ext.SDK/include/RED4ext/ISerializable.hpp`) já está CONFIRMADO por 2 classes
// independentes (`CMesh::PostLoad`/`MorphTargetMesh::PostLoad`, `postload-probe`/2026-07-25,
// `cp77-console/src/selftest.rs`) — dispatch aqui é GENÉRICO (lê o vtable do PRÓPRIO objeto em
// runtime), zero endereço por-classe, zero RE nova. Ver nota grande em
// `cp77-console/src/rtti.rs::iserializable_process_post_load`.
//
// ⚠️ Efeito real, não leitura pura: `PostLoad` reprocessa o objeto como recém-carregado do disco
// — mutação genuína. O smoke test deste item é deliberadamente OBSERVE-ONLY (só valida os
// guards de segurança contra ponteiro/vtable inválidos, nunca chama de verdade num objeto vivo
// automaticamente), mesma disciplina já usada pra `SetChunkMask`/`RefreshAppearance`/
// `ToggleGarageVehicle`. Item `#58` continua REAL_GAP até confirmação ao vivo.
native func BwmsProcessPostLoad(obj: ref<IScriptable>, disablePreInitialization: Bool) -> Bool

@addMethod(IScriptable)
public func ProcessPostLoad(opt disablePreInitialization: Bool) -> Bool {
    return BwmsProcessPostLoad(this, disablePreInitialization);
}

// Nome real da fonte (`ISerializable.reds:8-10`): `RefreshResource` é sugar puro sobre
// `ProcessPostLoad`, cópia fiel do corpo (`this.ProcessPostLoad(disablePreInitialization)`).
@addMethod(IScriptable)
public func RefreshResource(opt disablePreInitialization: Bool) -> Bool {
    return this.ProcessPostLoad(disablePreInitialization);
}
