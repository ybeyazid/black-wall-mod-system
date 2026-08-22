// red4ext-scriptref-proof.reds — RED4ext.SDK #323 (`ScriptRef<T>`, 2026-08-10, cont.192).
// Prova que `script_ref<T>` (passagem por referência do redscript) marshalha de verdade,
// lendo o wrapper real `{unk00[0x10], innerType@0x10, ref:T*@0x18, hash:CName@0x20}`
// (RED4ext.SDK NativeTypes.hpp/RTTITypes.hpp) em vez do workaround de registrar com `String`
// puro que `Utils/Hash.reds`/`Number.reds` usam desde 2026-07-13.

native func BwmsScriptRefStrEcho(data: script_ref<String>) -> String;
