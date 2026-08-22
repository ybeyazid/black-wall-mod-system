// archivexl-66-charactercustomizationhelper-haircolor.reds — ArchiveXL itens #66/#47
// (`CharacterCustomization.hpp` — `CharacterCustomizationHelper::GetHairColor`, "nunca antes
// nomeado", confirmado hoje mais cedo nesta mesma rodada de investigação como zero hit em
// `symbols-demangled.txt` — mesma categoria "nunca resolvido nem oficialmente pelo ArchiveXL").
//
// Achado (2026-08-12, round 5, agente dedicado, 100% offline): `GetHairColor` NÃO é `RawFunc`
// nenhuma pra este projeto — é leitura de CAMPO puro sobre a MESMA estrutura de opções de
// customização já usada (e já disparada ao vivo sem crash, `proofs/2026-07-28-axl-
// customization-apply-GetHeadOptions-PROVADO.log`) pelo item #47. Cadeia completa, zero RE de
// endereço, zero vtable:
//
//   `GameInstance.GetCharacterCustomizationSystem(game)
//       -> ref<gameuiICharacterCustomizationSystem>`      (native, `orphans.script:11543`)
//   `.GetHeadOptions() -> array<ref<CharacterCustomizationOption>>`
//       (native REAL, TIPADO — `characterCreationMenu.script:22`; a prova de 2026-07-28 usou
//       dispatch RTTI ad-hoc porque não tinha cruzado que o método já está decompilado e
//       diretamente chamável por sintaxe normal de redscript)
//
// Cada `CharacterCustomizationOption` tem `.bodyPart`(gameuiCharacterCustomizationPart)/
// `.info`(ref<gameuiCharacterCustomizationInfo>)/`.currIndex`(Uint32). A cor em si é campo
// NATIVO: `option.info as gameuiAppearanceInfo` dá `.definitions:
// [gameuiIndexedAppearanceDefinition]`, cada entrada com `.color: Color` — `option.currIndex`
// indexa a definição ATIVA. Zero endereço nativo, zero vtable, só cast+índice de array já
// tipados pelo compilador `scc`.
//
// 🐛 CORREÇÃO 2026-08-17 (revisão de código, achado real ANTES da prova ao vivo): a nota
// original desta rodada (2026-08-12) tinha lido errado `characterCreationBodyMorphMenu.
// script:1076-1081` (`GetSlotName`) — aquele método usa `Equals(uiSlot,n"hairstyle") ||
// Equals(uiSlot,n"hair_color")` só pra CATEGORIZAR as duas opções (estilo E cor) sob o rótulo
// genérico `"UI_Hairs"`, usado exclusivamente pro ENQUADRAMENTO DE CÂMERA (`RequestCameraChange`)
// — mesmo padrão do bloco vizinho (`skin_color`/`skin_type` → `"UI_Skin"`, `beard`/`beard_color`
// → `"UI_Jaw"`, sempre agrupando estilo+cor sob 1 categoria de câmera). NÃO identifica "esta é a
// opção de COR especificamente" — identifica "esta opção pertence à categoria Hairs (poderia ser
// o SELETOR DE CORTE ou o SELETOR DE COR, os dois batem o mesmo OR)`. O código original desta
// rodada copiava esse OR pra achar a cor e RETORNAVA na 1ª opção que batesse — se `"hairstyle"`
// (seletor de CORTE, não de cor) aparecer no array ANTES de `"hair_color"`, a função devolvia o
// dado (ou `empty`, se o cast pra `gameuiAppearanceInfo` falhar no tipo errado) da opção ERRADA,
// nunca chegando a examinar a opção `hair_color` real. Fix: casar só `uiSlot == n"hair_color"`
// (nome literal e auto-descritivo, mesmo padrão de `"skin_color"`/`"beard_color"` no mesmo
// método real — sempre o slot com `_color` no nome é o específico de cor).
//
// NÃO TESTADO AO VIVO (100% offline por instrução desta rodada). `GetHeadOptions` em si já
// disparou sem crash (2026-07-28), mas sem confirmar dado de aparência REAL (nota do próprio
// catálogo: "prova visual nunca completada") — este arquivo é a peça que faltava pra tentar
// essa confirmação visual num boot futuro. Itens #47/#66 permanecem REAL_GAP (regra de ouro).

public class BwmsCharacterCustomizationHelper {
    // Núcleo puro (testável sem GameInstance): dado o array de opções já obtido, acha a opção
    // de cor de cabelo e devolve a cor ATIVA. `Color` zerado (0,0,0,0) = não achou/sem dado.
    public static func GetHairColor(options: array<ref<CharacterCustomizationOption>>) -> Color {
        let empty: Color = new Color(Cast<Uint8>(0), Cast<Uint8>(0), Cast<Uint8>(0), Cast<Uint8>(0));
        let i: Int32 = 0;
        while i < ArraySize(options) {
            let opt: ref<CharacterCustomizationOption> = options[i];
            if IsDefined(opt) && Equals(opt.bodyPart, gameuiCharacterCustomizationPart.Head)
                && Equals(opt.info.uiSlot, n"hair_color") {
                let appearance: ref<gameuiAppearanceInfo> = opt.info as gameuiAppearanceInfo;
                if IsDefined(appearance) && Cast<Int32>(opt.currIndex) < ArraySize(appearance.definitions) {
                    return appearance.definitions[Cast<Int32>(opt.currIndex)].color;
                };
                return empty;
            };
            i += 1;
        };
        return empty;
    }

    // Conveniência ponta-a-ponta: resolve o singleton + busca as opções de cabeça + filtra.
    public static func GetPlayerHairColor(game: GameInstance) -> Color {
        let sys: ref<gameuiICharacterCustomizationSystem> = GameInstance.GetCharacterCustomizationSystem(game);
        if !IsDefined(sys) {
            return new Color(Cast<Uint8>(0), Cast<Uint8>(0), Cast<Uint8>(0), Cast<Uint8>(0));
        };
        return BwmsCharacterCustomizationHelper.GetHairColor(sys.GetHeadOptions());
    }
}
