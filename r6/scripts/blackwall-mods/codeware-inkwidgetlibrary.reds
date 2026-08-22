// codeware-inkwidgetlibrary.reds — Codeware `scripts/UI/Core/inkWidgetLibraryResource.reds` +
// `inkWidgetLibraryReference.reds` (catálogo `#94`, round 5 — 2026-08-12). `inkWidgetLibraryResource`
// é `importonly struct` VANILLA REAL (`redscript-src/orphans.script:39986`), já com
// `IsValid`/`GetPath`/`GetHash` nativos — só faltava `SetPath` (construção programática). Liga
// DIRETO ao sistema `InkSpawner` já fechado no ArchiveXL (`axl-inkspawner-apply`). Ver nota grande
// em `cp77-console/src/register.rs::tramp_inkwidgetlibraryresource_setpath`.
//
// Mesma disciplina do `codeware-inkwidgetref.reds`: native fica GLOBAL
// (`BwmsInkWidgetLibraryResourceSetPath`), `@addMethod` fica PURO REDSCRIPT delegando — evita a
// categoria de bug de `@addMethod` NATIVO sobre `struct` já documentada nesta sessão.

native func BwmsInkWidgetLibraryResourceSetPath(self: script_ref<inkWidgetLibraryResource>, path: ResRef) -> Void;

@addMethod(inkWidgetLibraryResource)
public static func SetPath(self: script_ref<inkWidgetLibraryResource>, path: ResRef) -> Void {
    BwmsInkWidgetLibraryResourceSetPath(self, path);
}

// Atalho de construção: `inkWidgetLibraryResource` é struct do jogo sem construtor exposto ao
// redscript, então o caminho é declarar uma vazia e preencher o path pela nossa native.
@addMethod(inkWidgetLibraryResource)
public static func Create(path: ResRef) -> inkWidgetLibraryResource {
    let recurso: inkWidgetLibraryResource;
    BwmsInkWidgetLibraryResourceSetPath(recurso, path);
    return recurso;
}

@addMethod(inkWidgetLibraryReference)
public static func Create(path: ResRef, item: CName) -> inkWidgetLibraryReference {
    return inkWidgetLibraryReference(inkWidgetLibraryResource.Create(path), item);
}
