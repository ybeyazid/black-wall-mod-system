//! vtabledump.rs — CET `DumpVTablesTask` (`PENDENCIAS-UNIFICADAS.md`/`CATALOGO-EXAUSTIVO-CET.md`
//! item `#44`, penúltimo REAL_GAP de CET depois do fechamento de `FunctionOverride` em cont.208):
//! técnica de descoberta de vtable→nome-de-classe por CONSTRUÇÃO REAL de instância, em vez de
//! offset-guessing/análise estática do binário.
//!
//! # Como o CET real faz isto (C++, `enablers/CET/src/scripting/GameDump.cpp`, ~80 linhas)
//!
//! `DumpVTablesTask::Run()` itera `CRTTISystem::types` (TODOS os tipos RTTI do jogo, ~15-27 mil)
//! via `for_each`; pra cada tipo Classe, aloca um bloco cru (`std::make_unique<char[]>`, **NÃO**
//! o allocator do motor — o comentário do autor original explica por quê: *"We aren't borrowing
//! the game's allocator on purpose because some classes have Abstract allocators and they
//! assert"*), chama `apType->Construct(memoria)`, lê os primeiros 8 bytes como ponteiro de
//! vtable, e registra `vtable -> "VT_"+nome` num mapa. Também sobe a cadeia `parent` de toda
//! classe achada, catalogando vtables HERDADAS. **Nunca chama `Destroy()`** no objeto construído
//! — comentário do autor: *"Lets just leak memory from nested objects for now, this is broken on
//! certain classes, havent determined why"* — o vazamento é escolha deliberada, não descuido.
//!
//! # O que o BWMS já tem pronto (mesmo padrão do achado que fechou `FunctionOverride`, cont.208)
//!
//! Todas as peças que a técnica do CET usa já são primitivas PROVADAS em produção no BWMS,
//! nenhuma RE nova:
//! - `Registry::get_native_types()` (vtbl+0x40) = o mesmo `CRTTISystem::types.ForEach` — já
//!   devolve o universo inteiro (~27718 tipos, baseline conhecido desde `dispatch_scriptable_tweaks`).
//! - `rtti::type_kind(ty)` = `IType::GetType()`, já usado pra filtrar por `ERTTIType::Class`
//!   (mesma composição que fechou Codeware `#50`, cont.142).
//! - `rtti::new_object_from_class(cls)` = EXATAMENTE `GetSize@0x18` + `GetAlignment@0x20` +
//!   `Construct@0x40`, os mesmos 3 slots de vtable que o CET chama — já em produção desde
//!   `new_object`/`MakeHandle` (Codeware `#51`), **incluindo o mesmo comportamento de VAZAR de
//!   propósito** (nunca chama Destruct — este projeto já tinha chegado na mesma conclusão do CET
//!   antes de ler o C++ dele: `new_object_from_class_named` não libera).
//! - `rtti::class_flags(cls)` (bit 0 = `isAbstract`) — o BWMS JÁ TEM o campo que explica a
//!   ressalva do comentário do CET. `new_object` (uso normal, 1 classe por chamada, escolhida a
//!   dedo pelo autor do teste) nunca precisou filtrar isso; dump EM MASSA precisa, porque
//!   construir uma classe abstrata de verdade é o próprio caso que o CET documenta como perigoso.
//!
//! # Por que isto NÃO é tão seguro quanto `FunctionOverride` foi (honestidade de escopo)
//!
//! O achado de `FunctionOverride` (cont.208) foi "risco ZERO adicional" porque reusava um hook
//! JÁ EXERCITADO por TODA chamada de função do jogo (`exec_replacement`) — nenhuma chamada nova
//! entrava em código nunca visitado. Este item é estruturalmente diferente: a técnica do CET
//! exige CONSTRUIR de verdade cada uma das ~15-27 mil classes do RTTI, a MAIORIA das quais o
//! BWMS NUNCA tentou instanciar (o histórico do projeto só provou `new_object` contra um punhado
//! de classes ESCOLHIDAS A DEDO — `PlayerPuppet`, `gameGodModeSystem`, classes forjadas pelo
//! próprio BWMS, `gamedataWeaponItem_Record`, etc. — nunca "qualquer uma das milhares"). O
//! próprio comentário do autor original do CET (acima) confirma que mesmo eles, rodando isso há
//! ANOS em produção contra o jogo Windows, sabem que uma fração das classes tem comportamento
//! imprevisível ao construir (asserts de allocator abstrato) e ao destruir ("broken on certain
//! classes, havent determined why") — ou seja, mesmo a fonte oficial não confia 100% nesta
//! operação em massa. Isso bate com o veredito já registrado nesta sessão do projeto
//! (`HISTORICO.md`, cluster ~cont.150-174): "construir instâncias em massa de tipos arbitrários é
//! genuinamente arriscado, categoria diferente dos reads/writes seguros já feitos" — item
//! DECLINADO uma vez por esse motivo exato, antes de qualquer filtro de `isAbstract` existir.
//!
//! # Design deste rascunho: converter "tudo de uma vez" em "bounded, filtrado, opt-in"
//!
//! Em vez de replicar literalmente o `for_each` do CET (1 chamada síncrona cobrindo o universo
//! inteiro, tudo-ou-nada), este módulo expõe 2 formas de uso deliberadamente mais estreitas,
//! pensadas pra nunca apostar o processo inteiro numa única leva de milhares de `Construct()`
//! nunca antes exercitados:
//!
//! 1. **`dump_one(reg, class_name)`** — 1 classe NOMEADA, exatamente o mesmo perfil de risco que
//!    o comando `newobj` já em produção (autor do teste escolhe a classe, não é iteração cega).
//! 2. **`dump_range(reg, start, count)`** — fatia BOUNDED do universo (`get_native_types()
//!    [start..start+count]`), pulando (a) tipos que não são `ERTTIType::Class`, (b) classes com
//!    `isAbstract` setado (`class_flags` bit 0 — a MESMA categoria que o comentário do CET aponta
//!    como perigosa), (c) ponteiros que falham a checagem `sane()`/`is_readable`. Um operador
//!    cobre o universo inteiro chamando isto MÚLTIPLAS VEZES com `start` crescente (ex. 100-200
//!    classes por chamada) — se uma leva crashar o boot, o range exato fica isolado a essa
//!    chamada, não ao processo inteiro; a próxima sessão retoma de onde parou em vez de repetir
//!    do zero.
//!
//! O filtro de abstrato mitiga uma categoria de risco REAL que o próprio autor do CET
//! documentou — não elimina todo risco (o "broken on certain classes, havent determined why" da
//! Destruct sugere que existem OUTRAS categorias de classe problemática além de abstrata, ainda
//! não caracterizadas; este módulo evita o problema da Destruct simplesmente NUNCA chamando-a,
//! igual ao CET), mas reduz a superfície pro caso mais concretamente já-avisado, e o bounding
//! garante que qualquer categoria residual de risco fica contida a um `count` pequeno por chamada
//! em vez do universo inteiro.
//!
//! # Status: RASCUNHO — só compile-check offline (`cargo build --release --lib`), NUNCA
//! exercitado num boot real. Antes de creditar como capacidade fechada: testar `dump_range` com
//! um `count` PEQUENO (20-50) primeiro, em vários boots separados, antes de tentar cobrir o
//! universo inteiro numa sessão só. Ponto de integração pendente (não aplicado neste rascunho):
//! comando de canal `vtabledump <start> <count>` no match gigante de `lib.rs` (mesmo padrão de
//! `reflenum`/`reflderived`) chamando `probe_vtabledump` abaixo.

use std::ffi::c_void;

use crate::rtti::{self, Registry};

/// 1 entrada do mapa vtable→classe, no mesmo espírito do `vtableMap.emplace(vtable, "VT_"+name)`
/// do CET (aqui sem o prefixo `VT_`/`VT_RTTI_` — o chamador decide como rotular achados
/// herdados-de-vtable-RTTI vs. achados-de-instância-construída, se algum dia precisar).
#[derive(Clone, Debug)]
pub struct VtableEntry {
    pub vtable_addr: u64,
    pub class_name: String,
}

/// Estatísticas de uma passada de `dump_range` — pra nunca reportar "N vtables achadas" sem
/// dizer quantas classes foram PULADAS e por quê (honestidade sobre o que foi/não foi exercitado,
/// mesmo princípio de "escopo honesto" já usado nos proofs do projeto).
#[derive(Clone, Copy, Debug, Default)]
pub struct DumpStats {
    pub scanned: usize,
    pub constructed: usize,
    pub skipped_non_class: usize,
    pub skipped_abstract: usize,
    pub skipped_unreadable: usize,
}

/// Lê o ponteiro de vtable dos primeiros 8 bytes de uma instância construída — mesma leitura
/// crua que o CET faz (`*reinterpret_cast<uintptr_t*>(pMemory.get())`), com checagem
/// `is_readable` antes (padrão do projeto: sempre checar antes de dereferenciar memória vinda de
/// uma alocação que não fizemos nós mesmos com `Vec`/`Box` Rust).
unsafe fn read_vtable_ptr(instance: *mut c_void) -> Option<u64> {
    if instance.is_null() || !crate::gum::is_readable(instance as *const c_void, 8) {
        return None;
    }
    Some((instance as *const u64).read_unaligned())
}

/// Constrói UMA instância de uma classe NOMEADA e devolve seu ponteiro de vtable. Mesmo perfil
/// de risco que `newobj <classe>` (já em produção): o autor do teste escolhe a classe a dedo, não
/// há iteração cega sobre o universo. Reusa `rtti::new_object` por completo — zero código de
/// construção novo.
pub unsafe fn dump_one(reg: &Registry, class_name: &str) -> Option<VtableEntry> {
    let instance = rtti::new_object(reg, class_name);
    let vtable_addr = read_vtable_ptr(instance)?;
    Some(VtableEntry {
        vtable_addr,
        class_name: class_name.to_string(),
    })
}

/// Fatia BOUNDED do universo RTTI (`get_native_types()[start..start+count]`), construindo só as
/// entradas que sobrevivem aos 3 filtros de segurança (ver cabeçalho do módulo). Nunca chama
/// Destruct (vazamento deliberado, igual ao CET) — o pool do motor (`ADDR_POOL_DEFAULT_ALLOC_
/// ALIGNED`, já usado por `new_object_from_class`) absorve isso pro caso de teste pontual; uma
/// varredura completa do universo (~27718 classes) via chamadas repetidas NÃO deveria virar
/// hábito de produção (é ferramenta de RE/diagnóstico, não capacidade pra rodar toda vez que o
/// mod carrega).
pub unsafe fn dump_range(reg: &Registry, start: usize, count: usize) -> (Vec<VtableEntry>, DumpStats) {
    let mut out = Vec::new();
    let mut stats = DumpStats::default();
    let (entries, total) = reg.get_native_types();
    if entries.is_null() || total == 0 {
        return (out, stats);
    }
    let arr = entries as *const *mut c_void;
    let end = (start + count).min(total);
    if start >= end {
        return (out, stats);
    }
    for i in start..end {
        stats.scanned += 1;
        let slot_ptr = arr.add(i);
        if !crate::gum::is_readable(slot_ptr as *const c_void, 8) {
            stats.skipped_unreadable += 1;
            continue;
        }
        let ty = *slot_ptr;
        if ty.is_null() || !rtti::sane(ty) {
            stats.skipped_unreadable += 1;
            continue;
        }
        if rtti::type_kind(ty) != Some(2) {
            // 2 = ERTTIType::Class (rtti::decode_ertti_type) — só classes têm Construct/vtable
            // no sentido que interessa aqui (fundamentais/enums/arrays não têm instância própria).
            stats.skipped_non_class += 1;
            continue;
        }
        if let Some(flags) = rtti::class_flags(ty) {
            if flags & 1 != 0 {
                // isAbstract (bit 0, `decode_class_flags`) — a MESMA categoria que o comentário
                // do CET aponta como perigosa ("some classes have Abstract allocators and they
                // assert"). Mitigação real de uma categoria de risco JÁ DOCUMENTADA pela fonte.
                stats.skipped_abstract += 1;
                continue;
            }
        }
        let instance = rtti::new_object_from_class(ty);
        let Some(vtable_addr) = read_vtable_ptr(instance) else {
            stats.skipped_unreadable += 1;
            continue;
        };
        let name_hash = rtti::type_name_getname(ty);
        let class_name =
            crate::cname::name_of(name_hash).unwrap_or_else(|| format!("(hash={name_hash:#x})"));
        stats.constructed += 1;
        out.push(VtableEntry {
            vtable_addr,
            class_name,
        });
    }
    (out, stats)
}

/// Comando `vtabledump <start> <count>` — ponto de entrada estilo `probe_*`. Gated atrás de
/// `crate::dev_mode()` por padrão do projeto pra qualquer capacidade em massa ainda sem prova ao
/// vivo (mesmo padrão de `install_reslink`/`mkflat`/`mkarr`) — só promover pra incondicional
/// depois de validar em boots reais com `count` pequeno.
pub unsafe fn probe_vtabledump(reg: &Registry, start: usize, count: usize) {
    if !crate::dev_mode() {
        crate::log(
            "[vtabledump] requer dev_mode (marcador /tmp/bwms-dev) — ferramenta de RE/diagnóstico em massa, não roda em produção",
        );
        return;
    }
    let (found, stats) = dump_range(reg, start, count);
    crate::log(&format!(
        "[vtabledump] range=[{start}..{}) scanned={} constructed={} skip(non_class={} abstract={} unreadable={})",
        start + count,
        stats.scanned,
        stats.constructed,
        stats.skipped_non_class,
        stats.skipped_abstract,
        stats.skipped_unreadable
    ));
    for e in found.iter().take(20) {
        crate::log(&format!("[vtabledump]   {:#018x} -> {}", e.vtable_addr, e.class_name));
    }
    if found.len() > 20 {
        crate::log(&format!(
            "[vtabledump]   ... +{} mais (não impressas, ver `found` no chamador se precisar de tudo)",
            found.len() - 20
        ));
    }
}

/// Codeware `#176` (Cyberpunk `WidgetInputService::ProcessCharacterEvent`/`inkWidget::TriggerEvent`
/// candidatos, `0x1049b1c64`/`0x1049bded0`) — sessão 2026-08-19 tarde: `dyld_info -fixups` achou
/// 32 slots `__DATA_CONST` guardando `0x1049b1c64` como ponteiro de vtable, mas NENHUMA vtable
/// própria do jogo tem símbolo RTTI (`_ZTV*`) no binário — bloqueio de "achar o NOME da classe
/// dona a partir do endereço da vtable". Esta função inverte a direção: em vez de procurar QUAL
/// vtable é, CONSTRÓI uma instância de uma classe NOMEADA conhecida (`dump_one`, já provado ao
/// vivo pelo CET item `#44`/`DumpVTablesTask` — 270 classes construídas, zero crash) e lê o
/// próprio ponteiro de vtable dela — ground-truth direto de memória, sem precisar de símbolo
/// nenhum. Se essa classe (ou uma classe-vizinha que herda o mesmo slot sem override — achado já
/// documentado hoje: o mesmo valor `0x1049b1c64` repete IDÊNTICO em pelo menos 2 tabelas
/// consecutivas) tiver esse valor num dos slots, confirma por CONSTRUÇÃO qual classe(s) usam esse
/// slot — sem depender de achar o "início" da região densa via scan de fixups.
///
/// Endurecido vs. `dump_one` (que não filtra `isAbstract`, mesmo perfil de risco que `newobj`
/// escolhido a dedo): aqui a classe pode vir de uma lista fixa de candidatos que o CHAMADOR não
/// necessariamente já verificou como concreta, então checa `class_flags` ANTES de construir —
/// mesma mitigação que `dump_range` já usa em escala.
pub unsafe fn dump_one_safe(reg: &Registry, class_name: &str) -> Result<VtableEntry, &'static str> {
    let cls = reg.class_by_name(class_name);
    if cls.is_null() {
        return Err("classe nao encontrada por nome (class_by_name devolveu null)");
    }
    if let Some(flags) = rtti::class_flags(cls) {
        if flags & 1 != 0 {
            return Err("classe abstrata (isAbstract) — pulada por seguranca, mesmo filtro do dump_range");
        }
    }
    let instance = rtti::new_object_from_class(cls);
    let Some(vtable_addr) = read_vtable_ptr(instance) else {
        return Err("instancia nula ou vtable ilegivel (is_readable falhou)");
    };
    Ok(VtableEntry {
        vtable_addr,
        class_name: class_name.to_string(),
    })
}

/// Lê `count` slots (qwords) a partir de um ponteiro de vtable JÁ RESOLVIDO em RUNTIME (ex. lido
/// do offset 0x00 de uma instância viva), devolvendo o **vmaddr ESTÁTICO** de cada slot (via
/// `crate::un_rebase`, mesmo padrão já usado por `questvtabledump`) — permite comparar direto
/// contra candidatos de RE já catalogados no binário (que são sempre endereços estáticos, não
/// runtime/ASLR-deslizados). `None` numa posição = leitura ilegível (para a varredura ali, não
/// finge um valor).
pub unsafe fn read_vtable_slots(vtable_addr: u64, count: usize) -> Vec<Option<u64>> {
    let mut out = Vec::with_capacity(count);
    let vt = vtable_addr as *const u8;
    for slot in 0..count {
        let sp = vt.add(slot * 8) as *const u64;
        if !crate::gum::is_readable(sp as *const c_void, 8) {
            out.push(None);
            continue;
        }
        let fn_rt = sp.read_unaligned();
        if fn_rt == 0 {
            out.push(Some(0));
            continue;
        }
        let fn_vm = crate::un_rebase(fn_rt as *const c_void);
        out.push(Some(fn_vm));
    }
    out
}

/// Comando `vtslots <ClassName> <count>` — constrói 1 instância NOMEADA (perfil de risco de
/// `newobj`, endurecido com o filtro `isAbstract`) e dumpa os primeiros `count` slots do vtable
/// dela como vmaddr estático, flagando visualmente qualquer slot que bata um dos 2 candidatos
/// conhecidos do item `#176` (`0x1049b1c64`/`0x1049bded0`). Gated `dev_mode()`, mesmo padrão do
/// `vtabledump`.
pub unsafe fn probe_vtslots(reg: &Registry, class_name: &str, count: usize) {
    if !crate::dev_mode() {
        crate::log("[vtslots] requer dev_mode (marcador /tmp/bwms-dev) — ferramenta de RE/diagnóstico, não roda em produção");
        return;
    }
    match dump_one_safe(reg, class_name) {
        Err(e) => {
            crate::log(&format!("[vtslots] {class_name}: FALHOU — {e}"));
        }
        Ok(entry) => {
            let vt_static = crate::un_rebase(entry.vtable_addr as *const c_void);
            crate::log(&format!(
                "[vtslots] {class_name} construida ok, vtable runtime={:#018x} static={:#018x}",
                entry.vtable_addr, vt_static
            ));
            let slots = read_vtable_slots(entry.vtable_addr, count);
            let mut matches = 0usize;
            for (i, s) in slots.iter().enumerate() {
                match s {
                    None => {
                        crate::log(&format!("[vtslots]   slot{i} ilegivel — parando aqui"));
                        break;
                    }
                    Some(fn_vm) => {
                        let flag = if *fn_vm == 0x1049b1c64 {
                            " <<<< MATCH candidato codeware-176 helper(0x1049b1c64)"
                        } else if *fn_vm == 0x1049bded0 {
                            " <<<< MATCH candidato codeware-176 TriggerEvent(0x1049bded0)"
                        } else {
                            ""
                        };
                        if !flag.is_empty() {
                            matches += 1;
                        }
                        crate::log(&format!("[vtslots]   slot{i} vmaddr={fn_vm:#010x}{flag}"));
                    }
                }
            }
            crate::log(&format!("[vtslots] {class_name}: {matches} match(es) nos candidatos do #176"));
        }
    }
}
