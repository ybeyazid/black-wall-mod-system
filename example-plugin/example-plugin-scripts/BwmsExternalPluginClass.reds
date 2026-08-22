// `red4ext-scripts-add` (2026-07-18, sessão `handle-ctor-re`, 13ª rodada) — este arquivo vive
// FORA de r6/scripts, ao lado do dylib do example-plugin (`red4ext/plugins/example-plugin-
// scripts/`), exatamente como um mod de 3os real que shipa dylib+reds juntos. O plugin
// (`bwms_plugin_main`) chama `api.scripts_add(<este path>)` — o BWMS registra o path num
// manifesto persistente (`~/.bwms-scripts-add.txt`); no PRÓXIMO ciclo compile+boot, o passo de
// compile (`bwms-fastboot.sh compile`) lê o manifesto e passa este arquivo pro `scc` via
// `-compilePathsFile`, ALÉM do `r6/scripts` normal — a classe abaixo passa a existir no RTTI
// sem NUNCA ter sido copiada pra dentro de `r6/scripts`.
public class BwmsExternalPluginClass {
    public func Ping() -> String {
        return "BWMS_SCRIPTS_ADD_EXTERNAL_PROOF";
    }
}
