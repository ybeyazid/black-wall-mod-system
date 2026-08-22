// -----------------------------------------------------------------------------
// Codeware.Reflection — atalho pragmático (cw-reflection-class-api, 2026-07-24)
// -----------------------------------------------------------------------------
//
// ✅ BISECÇÃO 2026-07-25 CONCLUÍDA (boot ao vivo, várias rodadas): isolado que este arquivo
// (sozinho, sem os outros 2) reproduzia o crash (EXC_BREAKPOINT/SIGTRAP em 0x103da2a60, "tipo de
// parâmetro não resolvido"). codeware-entity-builder.reds + codeware-ui-customcontroller.reds +
// ESTE arquivo (com o fix abaixo) bootam LIMPO JUNTOS — GAMEPLAY real confirmada (t=86s,
// autocontinue completo, zero crash) — LIBERADO pro bundle, sincronizado no jogo/zip/GitHub mirror.
//
// CAUSA-RAIZ ENCONTRADA (grep no projeto inteiro): este é o ÚNICO arquivo `.reds` de todo o
// projeto que combina `module X.Y` (linha `module Codeware.Reflection`) com `native func`
// declarado no MESMO escopo — todo outro arquivo com native func declara em escopo global (sem
// `module`), e os arquivos que usam `module` (ex. `codeware-ui-customcontroller.reds`,
// `module Codeware.UI`) não têm native func nenhuma. Hipótese: o compilador redscript qualifica o
// fullName de uma GLOBAL nativa declarada dentro de um `module` com o path do módulo (ex.
// `Codeware.Reflection::BwmsReflGetClass`), mas o registro Rust (`build_native_func`,
// register.rs) sempre usa o nome BARE — mesma classe de bug já documentada pra MÉTODOS de classe
// em `register_codeware_facade` (fullName com prefixo `Codeware::` quebrava o validador; fix foi
// usar o nome bare). Aqui a hipótese é o INVERSO: o bare pode estar faltando o qualificador que o
// bind-pass espera pra uma GLOBAL dentro de módulo.
//
// FIX: removida a linha `module Codeware.Reflection` (arquivo volta a declarar em escopo global,
// mesmo padrão de TODOS os outros arquivos de native func do projeto). Compila limpo, e o boot de
// confirmação final (2026-07-25, com Low Power Mode desligado — ver nota em
// `cp77-console/src/selfboot.rs::neutralize_engine_watchdog` sobre o crash SEPARADO que o LPM
// causava nos boots anteriores, não-relacionado a este arquivo) rodou os 3 arquivos JUNTOS até
// GAMEPLAY real, zero crash. `proof_needed` satisfeito literalmente.
//
// VEREDITO: fix CONFIRMADO por boot limpo ponta-a-ponta. Ver `HISTORICO.md` (2026-07-25).
//
// Fonte real (enablers/Codeware/scripts/Reflection/*.reds): Reflection/ERTTIType/
// ReflectionType/ReflectionClass/ReflectionFunc/ReflectionProp/ReflectionEnum são TODOS
// `native class`/`native struct` SEM parent vanilla nenhum na cadeia — diferente de
// `cw-localization` (que tinha `ScriptableSystem` real escondido no meio), aqui NÃO existe atalho
// de "estende algo vanilla" no nível da CLASSE. Forjar essas 5+ classes como `native class` fiel
// ao layout C++ real do Codeware seria o mesmo nível de risco do já feito pra Event/Target
// (RTTI-forge de tipo desconhecido) — correto continuar avaliando XL nesse sentido literal.
//
// O ATALHO REAL (achado pela investigação paralela do mesmo nome): praticamente toda operação-
// folha que `Reflection.reds` precisa JÁ FOI construída e provada in-game há sessões — só exposta
// hoje via comando de console (getf/setf/callf/rttidump/propdump) em vez de API redscript pra
// mods de 3os. Este arquivo embrulha os primitivos JÁ PROVADOS (register.rs, globais BwmsRefl*
// abaixo) em classes COMUNS (SEM `native`, SEM RegisterType nenhum) que guardam só um ponteiro
// opaco (`Uint64`) pro `CClass*`/`CProperty*`/`CBaseFunction*`/`CEnum*` real que o Rust já resolve
// — mesma categoria de segurança de `BWMSCheatHandler` (blackwall-mods.reds): zero RTTI-forge,
// zero risco de `class_validate_probe_hook`.
//
// Divergências documentadas (pragmáticas, mesmo estilo de `ResourceEvent.GetPath` — hash em vez
// de `ResRef` real, callbacksystem-native.reds):
// - `GetProperties() -> array<ref<ReflectionProp>>` vira `GetPropertyCount()` +
//   `GetPropertyByIndex(i)` — evita marshalar um `array<Uint64>` de retorno (RE nova, fora de
//   escopo desta rodada); mesmo dado, forma indexada em vez de array.
// - `ReflectionFunc.Call` só cobre invocação SEM ARGS (retorno truncado pra `Bool`) — `Variant`/
//   args arbitrários dependem de `cw-variant-marshalling`, gap L separado, genuinamente bloqueado
//   (nenhuma das 5 investigações desta rodada achou atalho pra ele).
// - `ReflectionClass.GetFunction` não distingue estático/instância (`resolve_in_class` já varre os
//   dois arrays — funcs@+0x48/staticFuncs@+0x58 — e devolve o 1º match por nome).
// - `GetType`/`GetInnerType`, `IsNative`/`IsAbstract`, `GetConstants` de bitfield e a família
//   enumerate-all (`GetClasses`/`GetTypes`/`GetDerivedClasses`/`GetEnums`/`GetBitfields`/
//   `GetGlobalFunctions`) ficam de fora — precisam achar o container interno de registro do
//   `CRTTISystem`, RE nova pequena não feita nesta rodada.
//
// NADA neste arquivo foi testado num boot ainda (nem as globais Rust, nem estas classes).

native func BwmsReflGetClass(name: CName) -> Uint64;
native func BwmsReflGetEnum(name: CName) -> Uint64;
native func BwmsReflGetGlobalFunction(name: CName) -> Uint64;
native func BwmsReflClassGetName(cls: Uint64) -> CName;
native func BwmsReflClassGetParent(cls: Uint64) -> Uint64;
native func BwmsReflClassGetProperty(cls: Uint64, name: CName) -> Uint64;
native func BwmsReflClassGetPropertyCount(cls: Uint64) -> Int32;
native func BwmsReflClassGetPropertyByIndex(cls: Uint64, idx: Int32) -> Uint64;
native func BwmsReflClassGetFunction(cls: Uint64, name: CName) -> Uint64;
native func BwmsReflPropGetName(prop: Uint64) -> CName;
native func BwmsReflPropGetValueFloat(obj: ref<IScriptable>, prop: Uint64) -> Float;
native func BwmsReflPropSetValueFloat(obj: ref<IScriptable>, prop: Uint64, value: Float) -> Bool;
native func BwmsReflPropGetValueBool(obj: ref<IScriptable>, prop: Uint64) -> Bool;
native func BwmsReflPropSetValueBool(obj: ref<IScriptable>, prop: Uint64, value: Bool) -> Bool;
native func BwmsReflFuncGetName(func: Uint64) -> CName;
native func BwmsReflFuncCall(func: Uint64, obj: ref<IScriptable>) -> Bool;
native func BwmsReflEnumGetValue(en: Uint64, name: CName) -> Uint64;
native func BwmsReflMakeHandle(cls: Uint64) -> Uint64;
// Codeware `#197` (`ReflectionType.GetInnerType()`, 2026-08-10, cont.192): tipo interno de um
// composto (array/handle/weak-handle). `typeName` é o nome literal do IType (ex.
// `n"array:Float"`/`n"handle:PlayerSystem"` — mesma convenção `"prefixo:Tipo"` já usada por
// `compose_params_from_types`/`GetType`). Devolve `n""` (hash 0) se não-composto ou não achado.
native func BwmsGetInnerTypeName(typeName: CName) -> CName;

public class ReflectionProp {
    private let m_ptr: Uint64;

    public func SetPtr(ptr: Uint64) -> Void {
        this.m_ptr = ptr;
    }
    public func GetPtr() -> Uint64 {
        return this.m_ptr;
    }
    public func IsValid() -> Bool {
        return this.m_ptr != 0ul;
    }

    public func GetName() -> CName {
        return BwmsReflPropGetName(this.m_ptr);
    }

    public func GetValueFloat(obj: ref<IScriptable>) -> Float {
        return BwmsReflPropGetValueFloat(obj, this.m_ptr);
    }
    public func SetValueFloat(obj: ref<IScriptable>, value: Float) -> Bool {
        return BwmsReflPropSetValueFloat(obj, this.m_ptr, value);
    }
    public func GetValueBool(obj: ref<IScriptable>) -> Bool {
        return BwmsReflPropGetValueBool(obj, this.m_ptr);
    }
    public func SetValueBool(obj: ref<IScriptable>, value: Bool) -> Bool {
        return BwmsReflPropSetValueBool(obj, this.m_ptr, value);
    }
}

public class ReflectionFunc {
    private let m_ptr: Uint64;

    public func SetPtr(ptr: Uint64) -> Void {
        this.m_ptr = ptr;
    }
    public func GetPtr() -> Uint64 {
        return this.m_ptr;
    }
    public func IsValid() -> Bool {
        return this.m_ptr != 0ul;
    }

    public func GetName() -> CName {
        return BwmsReflFuncGetName(this.m_ptr);
    }

    // Divergência documentada: só chamada SEM ARGS (ver nota de topo do arquivo).
    public func Call(obj: ref<IScriptable>) -> Bool {
        return BwmsReflFuncCall(this.m_ptr, obj);
    }
}

public class ReflectionEnum {
    private let m_ptr: Uint64;

    public func SetPtr(ptr: Uint64) -> Void {
        this.m_ptr = ptr;
    }
    public func GetPtr() -> Uint64 {
        return this.m_ptr;
    }
    public func IsValid() -> Bool {
        return this.m_ptr != 0ul;
    }

    public func GetValue(name: CName) -> Uint64 {
        return BwmsReflEnumGetValue(this.m_ptr, name);
    }
}

public class ReflectionClass {
    private let m_ptr: Uint64;

    public func SetPtr(ptr: Uint64) -> Void {
        this.m_ptr = ptr;
    }
    public func GetPtr() -> Uint64 {
        return this.m_ptr;
    }
    public func IsValid() -> Bool {
        return this.m_ptr != 0ul;
    }

    public func GetName() -> CName {
        return BwmsReflClassGetName(this.m_ptr);
    }

    public func GetParent() -> ref<ReflectionClass> {
        let parentPtr = BwmsReflClassGetParent(this.m_ptr);
        if parentPtr == 0ul {
            return null;
        }
        let result: ref<ReflectionClass> = new ReflectionClass();
        result.SetPtr(parentPtr);
        return result;
    }

    public func GetProperty(name: CName) -> ref<ReflectionProp> {
        let propPtr = BwmsReflClassGetProperty(this.m_ptr, name);
        if propPtr == 0ul {
            return null;
        }
        let result: ref<ReflectionProp> = new ReflectionProp();
        result.SetPtr(propPtr);
        return result;
    }

    // Divergência documentada: substitui `GetProperties()->array<ref<ReflectionProp>>` (ver nota
    // de topo do arquivo) por count+índice.
    public func GetPropertyCount() -> Int32 {
        return BwmsReflClassGetPropertyCount(this.m_ptr);
    }
    public func GetPropertyByIndex(idx: Int32) -> ref<ReflectionProp> {
        let propPtr = BwmsReflClassGetPropertyByIndex(this.m_ptr, idx);
        if propPtr == 0ul {
            return null;
        }
        let result: ref<ReflectionProp> = new ReflectionProp();
        result.SetPtr(propPtr);
        return result;
    }

    public func GetFunction(name: CName) -> ref<ReflectionFunc> {
        let funcPtr = BwmsReflClassGetFunction(this.m_ptr, name);
        if funcPtr == 0ul {
            return null;
        }
        let result: ref<ReflectionFunc> = new ReflectionFunc();
        result.SetPtr(funcPtr);
        return result;
    }

    // `MakeHandle()` (item #51, fechado 2026-08-09): devolve o ponteiro OPACO da instância nova
    // (Uint64, mesma convenção deste arquivo inteiro — não um `ref<T>` de verdade).
    public func MakeHandle() -> Uint64 {
        return BwmsReflMakeHandle(this.m_ptr);
    }
}

public class Reflection {
    public static func GetClass(name: CName) -> ref<ReflectionClass> {
        let clsPtr = BwmsReflGetClass(name);
        if clsPtr == 0ul {
            return null;
        }
        let result: ref<ReflectionClass> = new ReflectionClass();
        result.SetPtr(clsPtr);
        return result;
    }

    public static func GetEnum(name: CName) -> ref<ReflectionEnum> {
        let enPtr = BwmsReflGetEnum(name);
        if enPtr == 0ul {
            return null;
        }
        let result: ref<ReflectionEnum> = new ReflectionEnum();
        result.SetPtr(enPtr);
        return result;
    }

    public static func GetGlobalFunction(name: CName) -> ref<ReflectionFunc> {
        let funcPtr = BwmsReflGetGlobalFunction(name);
        if funcPtr == 0ul {
            return null;
        }
        let result: ref<ReflectionFunc> = new ReflectionFunc();
        result.SetPtr(funcPtr);
        return result;
    }
}
