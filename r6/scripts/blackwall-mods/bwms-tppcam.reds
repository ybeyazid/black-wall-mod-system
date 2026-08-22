// BWMS — Skill 1: VER-O-V (câmera 3ª pessoa autônoma), sem HID/Acessibilidade.
// [bisect save-load 2026-07-15] versão SEM trigger — só natives + as 4 classes DelayCallback.
// O start dos pollers (que estava em @wrapMethod) foi removido pra isolar se as CLASSES/NATIVES
// (footprint de load-time) crasham o world-load do save-load, ou se era só o trigger.

native func BwmsTppState() -> Int32;
native func BwmsCamBack() -> Int32;
native func BwmsCamX() -> Int32;
native func BwmsCamZ() -> Int32;
native func BwmsForceLook() -> Int32;
native func BwmsEquipState() -> Int32;
// BUG REAL achado 2026-07-17: este arquivo usa Print(...) (linhas de diagnóstico) mas NUNCA declarou
// `native func Print` — o bundle shipado compilava por acidente porque um .reds de TESTE/exemplo
// (fora do blackwall-mods-dev canônico) declarava Print e ficava compilado JUNTO. Sem esse arquivo
// externo presente, `scc -compile` no bundle oficial sozinho FALHA ("function 'Print' not found").
// Declarado aqui pra o bundle canônico compilar standalone, sem depender de nenhum arquivo externo.
native func Print(text: String) -> Void;

public class BwmsCamPoller extends DelayCallback {
  let m_game: GameInstance;

  public func Call() -> Void {
    let maxY: Float = Cast<Float>(BwmsCamBack()) / 100.0;
    let maxZ: Float = Cast<Float>(BwmsCamZ()) / 100.0;
    let player: ref<PlayerPuppet> = GameInstance.GetPlayerSystem(this.m_game).GetLocalPlayerControlledGameObject() as PlayerPuppet;
    if IsDefined(player) {
      let cam: ref<FPPCameraComponent> = player.GetFPPCameraComponent();
      if IsDefined(cam) {
        let pitch: Float = Matrix.GetRotation(cam.GetLocalToWorld()).Pitch;
        let startP: Float = -8.0;
        let fullP: Float = -60.0;
        let downF: Float = ClampF((startP - pitch) / (startP - fullP), 0.0, 1.0);
        cam.SetLocalPosition(new Vector4(0.0, maxY * downF, maxZ * downF, 1.0));
      };
    };
    // ACHADO 2026-07-15: reagendar um objeto NOVO (`new BwmsCamPoller()`) a cada Call() fazia o
    // poller disparar 1x e nunca mais — ao contrário do padrão REAL do motor
    // (`TemporalPrereqDelayCallback`, core/gameplay/prereqs/temporalPrereq.script), que reusa a
    // MESMA instância indefinidamente. FIX: reagenda `this` (mesmo objeto, campos já atuais).
    let ds: ref<DelaySystem> = GameInstance.GetDelaySystem(this.m_game);
    if IsDefined(ds) {
      ds.DelayCallback(this, 0.03);
    };
  }
}
native func BwmsEquipLog(step: Int32) -> Bool;
native func BwmsInvState() -> Int32;
native func BwmsEquipCheck() -> Int32;
native func BwmsEquipReadback(area: Int32, id: TweakDBID) -> Bool;
native func BwmsPollerTick(label: String, n: Int32) -> Void;

public class BwmsInvPoller extends DelayCallback {
  let m_game: GameInstance;
  let m_last: Int32;

  public func Call() -> Void {
    let want: Int32 = BwmsInvState();
    if want != this.m_last {
      if want == 1 {
        let ev: ref<inkMenuInstance_SpawnEvent> = new inkMenuInstance_SpawnEvent();
        ev.Init(n"OnSwitchToInventory");
        GameInstance.GetUISystem(this.m_game).QueueEvent(ev);
      };
      this.m_last = want;
    };
    // FIX 2026-07-15 (mesmo achado do BwmsCamPoller): reagenda `this`, não um objeto novo.
    let ds: ref<DelaySystem> = GameInstance.GetDelaySystem(this.m_game);
    if IsDefined(ds) {
      ds.DelayCallback(this, 0.3);
    };
  }
}

public class BwmsEquipPoller extends DelayCallback {
  let m_game: GameInstance;
  let m_last: Int32;
  let m_lastCheck: Int32;
  let m_calls: Int32;

  private func ItemFor(sel: Int32) -> TweakDBID {
    if sel == 1 { return t"Items.GOG_DLC_Jacket_Legendary"; };
    if sel == 2 { return t"Items.Fixer_01_Set_TShirt"; };
    if sel == 3 { return t"Items.Coat_04_rich_02_Crafting"; };
    return TDBID.None();
  }

  public func Call() -> Void {
    // CONTADOR (2026-07-15): registra a contagem EXATA de ciclos a cada 20 chamadas, pra correlacionar
    // o instante do crash por CICLO (não só tempo de parede) na próxima bisecção. O `/tmp/cp77-console.log`
    // continua no disco mesmo se o processo crashar logo depois — a última linha antes do crash dá o
    // nº de ciclos exato que o BwmsEquipPoller alcançou (a 0.3s/ciclo, ~1min55s ~= 383 ciclos esperados
    // se for tempo puro; se o nº real bater sempre no mesmo valor entre boots, confirma "por-ciclo").
    this.m_calls += 1;
    if this.m_calls % 20 == 0 {
      BwmsPollerTick("equip", this.m_calls);
    };
    let want: Int32 = BwmsEquipState();
    let player: ref<GameObject> = GameInstance.GetPlayerSystem(this.m_game).GetLocalPlayerControlledGameObject();
    if IsDefined(player) && want != this.m_last {
      if want > 0 {
        BwmsEquipLog(1);
        let id: TweakDBID = this.ItemFor(want);
        if TDBID.IsValid(id) {
          BwmsEquipLog(2);
          let req: ref<EquipRequest> = new EquipRequest();
          req.itemID = ItemID.FromTDBID(id);
          req.owner = player;
          req.addToInventory = true;
          req.slotIndex = -1;
          let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
          if IsDefined(es) {
            BwmsEquipLog(3);
            es.QueueRequest(req);
            BwmsEquipLog(4);
          };
        };
      };
      this.m_last = want;
    };
    let chk: Int32 = BwmsEquipCheck();
    if chk != this.m_lastCheck {
      if chk == 1 && IsDefined(player) {
        let ed: ref<EquipmentSystemPlayerData> = EquipmentSystem.GetData(player);
        if IsDefined(ed) {
          BwmsEquipReadback(1, ItemID.GetTDBID(ed.GetActiveItem(gamedataEquipmentArea.OuterChest)));
          BwmsEquipReadback(2, ItemID.GetTDBID(ed.GetActiveItem(gamedataEquipmentArea.InnerChest)));
          BwmsEquipReadback(3, ItemID.GetTDBID(ed.GetActiveItem(gamedataEquipmentArea.Legs)));
          BwmsEquipReadback(4, ItemID.GetTDBID(ed.GetActiveItem(gamedataEquipmentArea.Feet)));
        };
      };
      this.m_lastCheck = chk;
    };
    // FIX 2026-07-15 (mesmo achado do BwmsCamPoller): reagenda `this`, não um objeto novo.
    let ds: ref<DelaySystem> = GameInstance.GetDelaySystem(this.m_game);
    if IsDefined(ds) {
      ds.DelayCallback(this, 0.3);
    };
  }
}

// axl-garment-apply: gatilho de EQUIPAR item ÚNICO, SÍNCRONO, SEM agendamento (2026-07-29).
// `BwmsEquipPoller` acima tem crash PRÓPRIO documentado (~1min55s de execução sustentada via
// `ds.DelayCallback(this, 0.3)` recorrente, sessão 2026-07-15) — NÃO usar pra forçar equip
// pros hooks de "estado de item" do garment. Esta função reusa a MESMA lógica de `EquipRequest`
// já provada end-to-end (Skill 2, 2026-07-15), mas dispara 1 VEZ SÓ via `callg`/canal (comando
// `equiponce` em lib.rs), sem NENHUM DelayCallback/reagendamento — evita por completo o padrão
// temporal que causou o crash do poller (o crash era da INFRAESTRUTURA de poller recorrente, não
// do EquipRequest em si, que já rodou com sucesso quando não estava crashando).
public static func BwmsForceEquipOnce(game: GameInstance) -> Void {
  let want: Int32 = BwmsEquipState();
  if want <= 0 {
    Print("[equiponce] BwmsEquipState()<=0, nada a fazer (setar ~/.bwms-equip=1/2/3 antes)");
    return;
  };
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equiponce] player indefinido");
    return;
  };
  let id: TweakDBID = TDBID.None();
  if want == 1 { id = t"Items.GOG_DLC_Jacket_Legendary"; };
  if want == 2 { id = t"Items.Fixer_01_Set_TShirt"; };
  if want == 3 { id = t"Items.Coat_04_rich_02_Crafting"; };
  if !TDBID.IsValid(id) {
    Print("[equiponce] sel=" + ToString(want) + " sem item mapeado");
    return;
  };
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.FromTDBID(id);
  req.owner = player;
  req.addToInventory = true;
  req.slotIndex = -1;
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  if IsDefined(es) {
    es.QueueRequest(req);
    Print("[equiponce] EquipRequest enfileirado 1x, sem agendamento (sel=" + ToString(want) + ")");
  } else {
    Print("[equiponce] EquipmentSystem.GetInstance falhou");
  };
}

// axl-transmog-apply / axl-garment-apply (2026-08-02, pesquisa continuada via /goal): via NOVA,
// contorna `EquipmentSystem::QueueRequest` por completo em vez de tentar mitigar o crash dele.
// RE offline (mesma sessão) fechou `QueueRequest` como beco-sem-saída: `CClassFunction::GetInvokable()`
// retorna NULL pro descritor RTTI específico dessa função (bit4 de `unkAC`/+0xAC setado, hipótese forte
// = "requer despacho assíncrono via job/fila do motor, não invocável direto do interpretador" — ver
// HISTORICO.md 2026-08-02 cont.3-6). MAS a cadeia real do jogo (`equipmentSystem.script:839-948`,
// `EquipVisuals`→`ChangeAppearanceToItem`) mostra que a mutação visual em si roda numa função DIFERENTE,
// numa classe DIFERENTE: `TransactionSystem.ChangeItemAppearanceByItemID(obj, itemID, newItemID)`
// (`orphans.script:18053`, native PÚBLICA — não passa pelo `QueueRequest`/`GetInvokable()` quebrado,
// é um `CClassFunction` descriptor SEPARADO). `TransactionSystem` (`gameTransactionSystem`) já tem
// vtable mapeada no projeto (`0x1072173a0`, achada pra `GetVisualTags`, DATABASE.md) — categoria de
// função "consumidora" (não "Find"/resolver em mapa racy), mesma categoria já PROVADA segura várias
// vezes (`LoadAppearance`, `ReassembleAppearance`, `RegisterPart`, `GetVisualTags`). Hipótese: chamar
// isso direto pode disparar a cadeia de troca de aparência (incl. `AppearanceChanger::SelectAppearanceName`
// em C++, o hook pendente de `axl-transmog-apply`) SEM tocar `QueueRequest` nenhuma vez.
// AINDA NÃO TESTADO AO VIVO (memória do sistema crítica no momento em que foi escrito — `Pages free`
// ~5-6 mil, mesmo padrão que já causou crashes de boot documentados; só compilado+deployado, gated,
// igual ao `equiponce`). Comando de canal: `transmogtry` (lib.rs).
// FIX (2026-08-03): `BwmsTransmogTryOnce` (abaixo) crashou no 1º teste ao vivo de verdade —
// ANTES até do 1º `Print`, então antes de `ChangeItemAppearanceByItemID`. Suspeito:
// `EquipmentSystem.GetData(player)` + `ed.GetActiveItem(area)` (1-arg, método de
// `EquipmentSystemPlayerData`) é caminho NUNCA testado nesta sessão — `EquipmentSystem.GetActiveItem
// (owner, area)` (2-arg, direto na classe `EquipmentSystem`) já é PROVADO seguro (usado por
// `BwmsCheckEquipSlot`, chamado com sucesso repetidas vezes). Esta versão evita `GetData`/`ed`
// por completo, usando só chamadas já confirmadas seguras.
public static func BwmsTransmogTryV2(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[transmogtryv2] player indefinido");
    return;
  };
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  if !IsDefined(es) {
    Print("[transmogtryv2] EquipmentSystem.GetInstance falhou");
    return;
  };
  // OuterChest (jacket) transmogrificado pra parecer a T-shirt — visualmente BEM diferente
  // (ao contrário de InnerChest→T-shirt quando o InnerChest JÁ é a T-shirt, que é um no-op
  // visual e explica por que SelectAppearanceName não disparou na 1ª tentativa).
  let oldItem: ItemID = es.GetActiveItem(player, gamedataEquipmentArea.OuterChest);
  if !ItemID.IsValid(oldItem) {
    Print("[transmogtryv2] sem item ativo em OuterChest, nada pra transmogrificar");
    return;
  };
  let newItem: ItemID = ItemID.CreateQuery(t"Items.Fixer_01_Set_TShirt");
  let ts: ref<TransactionSystem> = GameInstance.GetTransactionSystem(game);
  if !IsDefined(ts) {
    Print("[transmogtryv2] GetTransactionSystem falhou");
    return;
  };
  Print("[transmogtryv2] pré-chamada: ts/player/oldItem/newItem prontos, chamando ChangeItemAppearanceByItemID AGORA");
  ts.ChangeItemAppearanceByItemID(player, oldItem, newItem);
  Print("[transmogtryv2] pós-chamada: ChangeItemAppearanceByItemID retornou, zero crash");
}

public static func BwmsTransmogTryOnce(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[transmogtry] player indefinido");
    return;
  };
  let ed: ref<EquipmentSystemPlayerData> = EquipmentSystem.GetData(player);
  if !IsDefined(ed) {
    Print("[transmogtry] EquipmentSystem.GetData falhou");
    return;
  };
  let oldItem: ItemID = ed.GetActiveItem(gamedataEquipmentArea.OuterChest);
  if !ItemID.IsValid(oldItem) {
    Print("[transmogtry] sem item ativo em OuterChest, nada pra transmogrificar (equipe algo antes)");
    return;
  };
  // Item-alvo diferente do atual, mesmo catálogo já usado em BwmsEquipPoller/BwmsForceEquipOnce
  // (sel=2, já confirmado válido no TweakDB deste save).
  let newId: TweakDBID = t"Items.Fixer_01_Set_TShirt";
  if !TDBID.IsValid(newId) {
    Print("[transmogtry] TDBID novo inválido");
    return;
  };
  let newItem: ItemID = ItemID.FromTDBID(newId);
  let ts: ref<TransactionSystem> = GameInstance.GetTransactionSystem(game);
  if !IsDefined(ts) {
    Print("[transmogtry] GetTransactionSystem falhou");
    return;
  };
  // Traço cirúrgico (2026-08-03): equiponce (baseline) confirma o ambiente saudável — precisa achar
  // se ChangeItemAppearanceByItemID é o ponto exato de crash (igual QueueRequest) ou se é antes.
  Print("[transmogtry] pré-chamada: ts/player/oldItem/newItem prontos, chamando ChangeItemAppearanceByItemID AGORA");
  ts.ChangeItemAppearanceByItemID(player, oldItem, newItem);
  Print("[transmogtry] pós-chamada: ChangeItemAppearanceByItemID retornou, zero crash");
}

// axl-garment-apply / axl-transmog-apply (2026-08-02, /goal): 2ª via nova, mais direta que a do
// `BwmsTransmogTryOnce` acima. RE dedicada desta sessão achou o endereço C++ CRU de
// `EquipmentSystem::QueueRequest` (0x103b1f624, herdado de `ScriptableSystem` — ver register.rs/
// HISTORICO.md 2026-08-02 cont.12). `BwmsQueueRequestRaw(sys, req)` (native Rust) chama esse endereço
// DIRETO via transmute, contornando por completo o dispatcher RTTI/`GetInvokable()` (a causa raiz
// EXATA do crash do `es.QueueRequest(req)` normal — RE em cont.3-6). Monta o MESMO `EquipRequest`
// já provado (Skill 2, 2026-07-15), mas despacha pela via raw em vez da chamada de método normal.
native func BwmsQueueRequestRaw(sys: ref<IScriptable>, req: ref<IScriptable>) -> Bool;

// 2026-08-03 (/goal): teste MÍNIMO/ISOLADO — `equiprawv2` crashou mesmo depois do fix de `ret_type`.
// Hipótese: `call_func` NUNCA invocou uma função redscript retornando `ref<T>` neste projeto antes
// (todo exemplo -> ref<T> só é chamado de DENTRO de outro script, nunca do nosso `call_func`) — pode
// ser essa combinação em si que é o problema, não `EquipmentSystem`/`GetPlayerSystem`. Esta função
// não faz NADA além de retornar `null` — isola a teoria por completo. Comando: `rettest`.
public static func BwmsTestReturnsRef(game: GameInstance) -> ref<IScriptable> {
  Print("[rettest] BwmsTestReturnsRef chamado, retornando null");
  return null;
}

// Isolamento por etapas (2026-08-03): `rettest` (acima) PASSOU (zero crash) — refuta a teoria de
// "call_func→redscript→ref<T> é sempre quebrado". O bug tem que estar em algo específico da cadeia
// GetPlayerSystem/EquipmentSystem.GetInstance, só quando a função externa não é Void. Isolando
// incrementalmente: rettest2=só GetPlayerSystem · rettest3=+GetLocalPlayerControlledGameObject ·
// rettest4=+EquipmentSystem.GetInstance(descarta, retorna null) · rettest5=retorna es DE VERDADE.
public static func BwmsTestRef2(game: GameInstance) -> ref<IScriptable> {
  let ps: ref<PlayerSystem> = GameInstance.GetPlayerSystem(game);
  Print("[rettest2] GetPlayerSystem ok=" + ToString(IsDefined(ps)) + ", retornando null");
  return null;
}

public static func BwmsTestRef3(game: GameInstance) -> ref<IScriptable> {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  Print("[rettest3] player ok=" + ToString(IsDefined(player)) + ", retornando null");
  return null;
}

public static func BwmsTestRef4(game: GameInstance) -> ref<IScriptable> {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  Print("[rettest4] es ok=" + ToString(IsDefined(es)) + ", DESCARTANDO, retornando null");
  return null;
}

public static func BwmsTestRef5(game: GameInstance) -> ref<IScriptable> {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  Print("[rettest5] es ok=" + ToString(IsDefined(es)) + ", retornando ES DE VERDADE agora");
  return es;
}

// 2026-08-03 (/goal, achado ao vivo): `equipctxonce`/`equiprawonce` (via `BwmsQueueRequestCtx`/`Raw`
// chamados de DENTRO do bytecode de `BwmsForceEquipCtx`/`Raw`) crasham no MESMO endereço de sempre
// (`0x1021730ec`) ANTES de qualquer log nosso aparecer — nem o Print imediatamente antes da chamada,
// nem o log de ENTRADA do trampolim Rust. Isso aponta pro DESPACHO do native (2 params `handle`)
// quando invocado ANINHADO de bytecode já em execução como o problema, não a lógica interna dele.
// **Via nova: eliminar o aninhamento por completo.** Estas 2 funções só CONSTROEM e RETORNAM `es`/`req`
// (zero chamada a native de 2-handle-params dentro delas) — chamadas TOP-LEVEL, separadas, direto do
// Rust (mesmo padrão já provado de `give`/`GetGame`). O `transmute` pro endereço cru roda inteiramente
// do lado Rust (`run_cmd`), sem NENHUM native aninhado no meio. Comando de canal: `equiprawv2`.
public static func BwmsPrepEquipSys(game: GameInstance) -> ref<EquipmentSystem> {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equiprawv2] BwmsPrepEquipSys: player indefinido");
    return null;
  };
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  if !IsDefined(es) {
    Print("[equiprawv2] BwmsPrepEquipSys: EquipmentSystem.GetInstance falhou");
  };
  return es;
}

public static func BwmsPrepEquipReq(game: GameInstance) -> ref<EquipRequest> {
  let want: Int32 = BwmsEquipState();
  if want <= 0 {
    Print("[equiprawv2] BwmsPrepEquipReq: BwmsEquipState()<=0");
    return null;
  };
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equiprawv2] BwmsPrepEquipReq: player indefinido");
    return null;
  };
  let id: TweakDBID = TDBID.None();
  if want == 1 { id = t"Items.GOG_DLC_Jacket_Legendary"; };
  if want == 2 { id = t"Items.Fixer_01_Set_TShirt"; };
  if want == 3 { id = t"Items.Coat_04_rich_02_Crafting"; };
  if !TDBID.IsValid(id) {
    Print("[equiprawv2] BwmsPrepEquipReq: sel=" + ToString(want) + " sem item mapeado");
    return null;
  };
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.FromTDBID(id);
  req.owner = player;
  req.addToInventory = true;
  req.slotIndex = -1;
  Print("[equiprawv2] BwmsPrepEquipReq: req pronto, sel=" + ToString(want));
  return req;
}

// equiprawv5 (2026-08-03): verificação DEFINITIVA se `QueueRequest` (via frame real, bypass
// GetInvokable) de fato adicionou/equipou o item — lê a quantidade real via TransactionSystem,
// a mesma API pública já usada por `give`/`transmogtry`. Comando de canal: `callg BwmsCheckItemQty()`.
public static func BwmsCheckItemQty(game: GameInstance) -> Int32 {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equiprawv5-check] player indefinido");
    return -1;
  };
  let ts = GameInstance.GetTransactionSystem(game);
  if !IsDefined(ts) {
    Print("[equiprawv5-check] GetTransactionSystem falhou");
    return -2;
  };
  // FIX (2026-08-03): `FromTDBID` gera um SEED ALEATÓRIO novo a cada chamada (ItemID = TDBID +
  // seed de instância, não só o TDBID) — comparar contra isso NUNCA bate com o que o player
  // realmente possui. `CreateQuery` é o ID "curinga" (sem seed específico) certo pra consultas
  // de posse/quantidade por TIPO, independente de qual instância aleatória foi dada.
  let id: ItemID = ItemID.CreateQuery(t"Items.Fixer_01_Set_TShirt");
  let qty: Int32 = ts.GetItemQuantity(player, id);
  Print("[equiprawv5-check] qty=" + ToString(qty));
  return qty;
}

// equiprawv6 (2026-08-03): `BwmsCheckItemQty` (TransactionSystem) ficou em 0 mesmo com
// `equiprawv6` retornando ZERO CRASH via ADDR_EXEC completo e todos os campos de req
// confirmados byte-exatos (itemID/owner/addToInventory/slotIndex, ver dump [equiprawv6-diag]).
// Checagem alternativa: `EquipmentSystem.IsEquipped` lê o estado REAL de equipamento
// (EquipmentSystemPlayerData), independente do TransactionSystem — T-shirt = slot InnerChest.
// Comando de canal: `callg BwmsCheckEquipSlot()`.
public static func BwmsCheckEquipSlot(game: GameInstance) -> Bool {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equiprawv6-check] player indefinido");
    return false;
  };
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  if !IsDefined(es) {
    Print("[equiprawv6-check] EquipmentSystem.GetInstance falhou");
    return false;
  };
  let id: ItemID = ItemID.CreateQuery(t"Items.Fixer_01_Set_TShirt");
  let equipped: Bool = es.IsEquipped(player, id, gamedataEquipmentArea.InnerChest);
  let active: ItemID = es.GetActiveItem(player, gamedataEquipmentArea.InnerChest);
  let activeIsTShirt: Bool = ItemID.IsOfTDBID(active, t"Items.Fixer_01_Set_TShirt");
  let activeValid: Bool = ItemID.IsValid(active);
  Print("[equiprawv6-check] equipped=" + ToString(equipped) + " activeValid=" + ToString(activeValid) + " activeIsTShirt=" + ToString(activeIsTShirt));
  return equipped;
}

// equiprawv7 (2026-08-03): achado-chave — o padrão CANÔNICO real (`equipAction.script:6-9`,
// `EquipAction.CompleteAction`) só seta `itemID`+`owner` (NUNCA `addToInventory`/`slotIndex`) e
// chama `EquipmentSystem.GetInstance(obj).QueueRequest(req)` DIRETO via bytecode — não via
// transmute/frame sintético. Isso só nunca funcionou porque `GetInvokable()` retorna null (fix
// já instalado por `equiprawv6` antes desta chamada, mesmo boot). Segunda causa de falha achada
// agora: `ItemID.FromTDBID` gera um SEED ALEATÓRIO novo a cada chamada — o item dado por `give`
// (outro seed) nunca bate com um itemID recém-sintetizado. `ItemID.CreateQuery` é o ID "curinga"
// certo pra equipar QUALQUER instância possuída do tipo, replicando o padrão real de UI (equipar
// por tipo, não por instância exata). Requer que o player JÁ POSSUA o item (via `give` antes).
public static func BwmsEquipViaQuery(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equiprawv7] player indefinido");
    return;
  };
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.CreateQuery(t"Items.Fixer_01_Set_TShirt");
  req.owner = player;
  Print("[equiprawv7] req pronto (padrão canônico, só itemID+owner) — chamando QueueRequest via bytecode...");
  EquipmentSystem.GetInstance(player).QueueRequest(req);
  Print("[equiprawv7] QueueRequest retornou — ZERO CRASH, via bytecode canônica completa!");
}

// codeware-213-complex-outfit (2026-08-19): item #213/VisualController — depCount/looseDepCount
// (appearanceDependency) seguem 0 com a TShirt (item de teste simples demais, achado explicado em
// codeware-visualcontroller-smoke.reds). Este helper equipa um item MAIS RICO (Coat_04_rich_02_
// Crafting — TweakDB confirmado: tag "Rich"/"Streetwear", 2 appearanceSuffixes vs 1 da TShirt,
// mais provável de ter dependência de aparência/garment real populada). Diferente de
// `BwmsEquipViaQuery` (usa `ItemID.CreateQuery`, exige posse PRÉVIA — só funciona pra itens já no
// inventário, como a TShirt inicial), este item NÃO é possuído por padrão — usa o padrão já
// canônico e provado de `BwmsEquipPoller`/`BwmsForceEquipOnce` (`bwms-tppcam.reds` acima):
// `ItemID.FromTDBID` (sintetiza ItemID novo, não exige posse) + `req.addToInventory=true` (dá o
// item automaticamente como parte do próprio EquipRequest).
public static func BwmsEquipComplexOutfitViaQuery(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equipcomplex] player indefinido");
    return;
  };
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.FromTDBID(t"Items.Coat_04_rich_02_Crafting");
  req.owner = player;
  req.addToInventory = true;
  req.slotIndex = -1;
  Print("[equipcomplex] req pronto (Coat_04_rich_02_Crafting, FromTDBID+addToInventory=true) — chamando QueueRequest via bytecode...");
  EquipmentSystem.GetInstance(player).QueueRequest(req);
  Print("[equipcomplex] QueueRequest retornou — ZERO CRASH");
}

// item #54 (2026-08-18, "tentativa 4" — a recomendação explícita deixada pela tentativa 3, ver
// `archivexl-54-scanspawningslots-smoke.reds`): as 3 tentativas anteriores de disparar
// `IsSlotSpawningAnyItem` nunca observaram uma transição GENUÍNA vazio->ocupado — a tentativa 2
// mirou um item GOG-exclusivo (no-op silencioso nesta instalação Steam) e a tentativa 3 mirou um
// item que JÁ estava ativo desde o aquecimento do boot (re-equip IDEMPOTENTE, sem transição real
// de estado). Pra forçar uma transição real, este helper DESEQUIPA primeiro (`UnequipRequest`,
// mesma família `PlayerScriptableSystemRequest`/`IScriptable` já usada com segurança por
// `EquipRequest`/`EquipVisualsRequest` nesta mesma sessão — zero categoria de risco nova, nunca
// declara classe própria, só instancia+seta campos numa classe VANILLA já existente e despacha
// via `EquipmentSystem.QueueRequest`, o mesmo bytecode-dispatch canônico já provado dezenas de
// vezes). `UnequipRequest` é chaveada por ÁREA (`areaType: gamedataEquipmentArea`), não por
// itemID — `slotIndex` fica no default `-1` (`@default(UnequipRequest, -1)` na fonte real,
// preenchido automaticamente pelo compilador ao instanciar) e `force` fica `false` (default
// implícito de `Bool`), replicando o caminho comum de desequipar pela UI (sem forçar remoção de
// item "preso"). Alvo = `InnerChest` (a mesma área da T-shirt já usada em `BwmsEquipViaQuery`,
// mantendo o par equip/unequip no MESMO item pra comparação direta).
public static func BwmsUnequipViaQuery(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[unequiprawv1] player indefinido");
    return;
  };
  let req: ref<UnequipRequest> = new UnequipRequest();
  req.owner = player;
  req.areaType = gamedataEquipmentArea.InnerChest;
  req.force = false;
  Print("[unequiprawv1] req pronto (UnequipRequest, owner+InnerChest, force=false) — chamando QueueRequest via bytecode...");
  EquipmentSystem.GetInstance(player).QueueRequest(req);
  Print("[unequiprawv1] QueueRequest retornou — ZERO CRASH, via bytecode canônica completa!");
}

// axl-29-equipcyber (2026-08-14): teste DECISIVO do item #29 (`ComputePuppetArmsState`,
// archivexl-puppetstate-armsdetect.reds) — mesmo padrão canônico do `equiprawv7`
// (itemID via CreateQuery + owner, QueueRequest via bytecode, fix de GetInvokable já instalado
// globalmente), mas com `Items.MantisBlades` (cyberware de braço REAL, TDBID confirmado offline
// contra `Items.MantisBlades.itemType == ItemType.Cyb_MantisBlades`, `cp77-symbols/tweakdb-base.tsv`)
// no lugar da T-shirt. Precisa de `give Items.MantisBlades` ANTES (posse real via `GiveItem`,
// mesmo requisito documentado desde 2026-08-03 — `CreateQuery` só resolve item já possuído).
public static func BwmsEquipCyberwareViaQuery(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equipcyber] player indefinido");
    return;
  };
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.CreateQuery(t"Items.MantisBlades");
  req.owner = player;
  Print("[equipcyber] req pronto (Items.MantisBlades, padrão canônico) — chamando QueueRequest via bytecode...");
  EquipmentSystem.GetInstance(player).QueueRequest(req);
  Print("[equipcyber] QueueRequest retornou — ZERO CRASH, via bytecode canônica completa!");
}

// axl-29-drawcyber (2026-08-18): teste DECISIVO #2 do item ArchiveXL #29 — achado honesto de
// 2026-08-14 (proofs/2026-08-14-archivexl-29-armsdetect-DECISIVO-tentado-INCONCLUSIVO.log)
// confirmou que `equipcyber` (EquipRequest) MOVE o item pra `gamedataEquipmentArea.RightArm`
// (posse/instalação real, confirmado via `scanslots`) mas NÃO popula `AttachmentSlots.
// WeaponRight` (a slot que `ComputePuppetArmsState` lê) — essa slot só reflete a arma
// DESEMBAINHADA/em uso, estado distinto de "instalado". Lendo a fonte real
// (`cyberpunk/systems/equipmentSystem.script:2157`, `EquipmentSystemPlayerData::DrawItem`)
// achei o mecanismo REAL que popula essa slot: monta um `EquipmentSystemWeaponManipulationRequest`
// internamente e chama `SetSlotActiveItem(EquipmentManipulationRequestSlot.Right, itemToDraw)`
// (caso `gamedataEquipmentArea.ArmsCW`, exatamente a área do Mantis Blades). O request
// REDSCRIPT-facing que dispara isso é `DrawItemRequest{itemID, owner, equipAnimationType}`
// (`orphans.script:38763`, handler `OnDrawItemRequest` → `this.DrawItem(request.itemID,...)`),
// despachado pelo MESMO `EquipmentSystem.QueueRequest` bytecode canônico já provado seguro (zero
// crash) por `EquipRequest`/`EquipVisualsRequest`/`UnequipRequest` nesta e em sessões anteriores
// (fix de GetInvokable já instalado globalmente, idempotente). Direcionado por ITEM ID
// específico (`Items.MantisBlades`, não "1º disponível"/"último usado" — ambos ambíguos contra
// o inventário real do save) — zero ambiguidade sobre QUAL arma é sacada.
//
// Global (não método de classe bare) DE PROPÓSITO — achado de metodologia de 2026-08-14: classes
// redscript "bare" com só métodos static (`public class X { static func... }`, sem parent
// nativo) não são enumeráveis via `resolve_in_class`/`funclistdump`, então não são re-chamáveis
// via console depois do 1º `OnGameAttached`. Globais soltas (`register::get_function`+`callg`,
// mesmo padrão já provado 100% por `equipcyber`/`scanslots`/`scanspawn`) não sofrem disso.
public static func BwmsDrawCyberwareAndCheckArms(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[drawcyber] player indefinido");
    return;
  };
  let req: ref<DrawItemRequest> = new DrawItemRequest();
  req.itemID = ItemID.CreateQuery(t"Items.MantisBlades");
  req.owner = player;
  req.equipAnimationType = gameEquipAnimationType.Default;
  Print("[drawcyber] DrawItemRequest pronto (Items.MantisBlades) — chamando QueueRequest via bytecode...");
  EquipmentSystem.GetInstance(player).QueueRequest(req);
  Print("[drawcyber] QueueRequest retornou — ZERO CRASH, via bytecode canônica completa!");
  let state: PuppetArmsState = ArchiveXLPuppetState.ComputePuppetArmsState(player);
  // Marcador 9613 (distinto de 9611/9612 já usados pelo smoke test original do item #29) —
  // sucesso da chamada em si (não confundir com o resultado do enum, linha seguinte).
  BwmsVariantLog(ToVariant(true), ToVariant(9613));
  BwmsVariantLog(ToVariant(true), ToVariant(EnumInt(state)));
}

// axl-transmog-apply (2026-08-05): mesmo padrão canônico do equiprawv7 (itemID+owner, QueueRequest
// via bytecode, fix de GetInvokable já instalado globalmente pra QUALQUER request que passe por
// EquipmentSystem.QueueRequest — não é específico de EquipRequest), mas com `EquipVisualsRequest`
// em vez de `EquipRequest`. `OnEquipVisualsRequest` (equipmentSystem.script:3502) chama
// `this.EquipVisuals(request.itemID)` direto — dispara `ChangeAppearanceToItem` sem precisar da UI
// real de Wardrobe/espelho, testável 100% pelo canal. Requer que o item já esteja EQUIPADO num
// slot ocupado (senão ChangeAppearanceToItem cai no ramo GivePreview, não no de transmog real).
public static func BwmsVisualEquipViaQuery(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[transmogviaquery] player indefinido");
    return;
  };
  let req: ref<EquipVisualsRequest> = new EquipVisualsRequest();
  req.itemID = ItemID.CreateQuery(t"Items.Fixer_01_Set_TShirt");
  req.owner = player;
  Print("[transmogviaquery] req pronto (EquipVisualsRequest, itemID+owner) — chamando QueueRequest via bytecode...");
  EquipmentSystem.GetInstance(player).QueueRequest(req);
  Print("[transmogviaquery] QueueRequest retornou — ZERO CRASH");
}

// Task #15 (2026-08-03): equipar em `Face` (óculos) em vez de `InnerChest` — slot tipicamente
// VAZIO no V base (ao contrário de InnerChest/OuterChest, que o save já tem algo), pra testar se
// `GarmentAssemblerState::AddItem` (nunca disparado ainda, só `ChangeItem`/`RemoveItem`) dispara
// quando o slot-alvo é genuinamente novo. `Items.Glasses_01_basic_01` — item real do TweakDB
// (achado via `tweakdb-tool find Glasses`), nunca usado neste projeto antes.
// Task #17 (2026-08-03): `AddItem` ainda sem confirmação — hipótese nova: InnerChest/OuterChest
// (categoria "clothing", dispararam Change/RemoveItem) vs Head (categoria "acessório", disparou
// ChangeCustomItem) sugerem que a escolha Add/Change/Custom depende da CATEGORIA do slot, não só
// da posse. `Outfit` (área 27) é o único slot tipo-clothing genuinamente vazio achado via
// `scanslots` — testando com o mesmo item de jaqueta já catalogado (`Items.GOG_DLC_Jacket_Legendary`).
public static func BwmsEquipOutfitViaQuery(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equipoutfit] player indefinido");
    return;
  };
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  let before: ItemID = es.GetActiveItem(player, gamedataEquipmentArea.Outfit);
  Print("[equipoutfit] antes: Outfit válido=" + ToString(ItemID.IsValid(before)));
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.CreateQuery(t"Items.GOG_DLC_Jacket_Legendary");
  req.owner = player;
  Print("[equipoutfit] req pronto (Outfit, jaqueta) — chamando QueueRequest via bytecode...");
  es.QueueRequest(req);
  Print("[equipoutfit] QueueRequest retornou — ZERO CRASH!");
}

public static func BwmsEquipGlassesViaQuery(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equipglasses] player indefinido");
    return;
  };
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  // Achado (scanslots): Face JÁ tinha algo; Head/Gadget/PersonalLink/QuickSlot/Splinter/
  // PlayerTattoo/SilverhandArm/LeftArm/Outfit estão genuinamente VAZIOS neste save. Tentando
  // Head (chapéu) com o item de óculos mesmo — se o slot não bater, deve ser no-op seguro
  // (mesmo comportamento observado até agora: mismatch nunca crashou, só não fez efeito).
  // FIX (2026-08-03): óculos em Head não disparou NENHUM hook (0/5) — provavelmente mismatch de
  // TIPO de item rejeitado antes de chegar no garment assembler. `Items.Hat_01_basic_01` é o item
  // REAL (achado via tweakdb-tool find "Items.Hat_") pro slot Head.
  let before: ItemID = es.GetActiveItem(player, gamedataEquipmentArea.Head);
  Print("[equipglasses] antes: Head válido=" + ToString(ItemID.IsValid(before)));
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.CreateQuery(t"Items.Hat_01_basic_01");
  req.owner = player;
  Print("[equipglasses] req pronto (Head, item novo, HAT desta vez) — chamando QueueRequest via bytecode...");
  es.QueueRequest(req);
  Print("[equipglasses] QueueRequest retornou — ZERO CRASH!");
}

// Task #15 (2026-08-03, achado: Face NÃO estava vazio, hipótese refutada) — varre várias áreas
// de uma vez (read-only, zero risco) pra achar uma genuinamente vazia sem gastar mais boots
// tentando um item por vez às cegas.
public static func BwmsScanEmptySlots(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[scanslots] player indefinido");
    return;
  };
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  let areas: array<gamedataEquipmentArea>;
  ArrayPush(areas, gamedataEquipmentArea.Head);
  ArrayPush(areas, gamedataEquipmentArea.Gadget);
  ArrayPush(areas, gamedataEquipmentArea.PersonalLink);
  ArrayPush(areas, gamedataEquipmentArea.QuickSlot);
  ArrayPush(areas, gamedataEquipmentArea.Splinter);
  ArrayPush(areas, gamedataEquipmentArea.PlayerTattoo);
  ArrayPush(areas, gamedataEquipmentArea.SilverhandArm);
  ArrayPush(areas, gamedataEquipmentArea.LeftArm);
  ArrayPush(areas, gamedataEquipmentArea.RightArm);
  ArrayPush(areas, gamedataEquipmentArea.Legs);
  ArrayPush(areas, gamedataEquipmentArea.Feet);
  ArrayPush(areas, gamedataEquipmentArea.Outfit);
  let i: Int32 = 0;
  while i < ArraySize(areas) {
    let it: ItemID = es.GetActiveItem(player, areas[i]);
    Print("[scanslots] area=" + ToString(EnumInt(areas[i])) + " valido=" + ToString(ItemID.IsValid(it)));
    i += 1;
  };
}

// ArchiveXL `#54` (AttachmentSlots.IsSlotEmpty/IsSlotSpawning) — sessão dedicada 2026-08-12
// (catálogo exaustivo, `CATALOGO-EXAUSTIVO-ARCHIVEXL.md`). "Vazio?" já foi respondido acima
// (`BwmsScanEmptySlots`, via `EquipmentSystem.GetActiveItem`+`ItemID.IsValid`, 2026-08-03) —
// mas "vazio" != "spawnando" (estado TRANSIENTE distinto: um item sendo instanciado/anexado ao
// slot, ainda não considerado "ocupado" pelo `GetActiveItem`, mas também não mais "vazio" de
// verdade). Achado (leitura de `cp77-symbols/redscript-src/orphans.script:18100-18157`, dentro
// da MESMA classe `TransactionSystem extends ITransactionSystem` já usada em outros mods deste
// projeto): `IsSlotSpawningAnyItem(obj: ref<GameObject>, slotID: TweakDBID) -> Bool` é NATIVA
// VANILLA já exposta ao redscript (`public final native func`, linha 18157) — a MESMA capacidade
// prática de `Raw::AttachmentSlots::IsSlotSpawning` (RawFunc do ArchiveXL, endereço Mac nunca
// achado nem por RE dedicada), via um caminho de mais alto nível que o motor já expõe sem
// precisar de RE nenhuma (zero endereço nativo, zero hook — mesmo padrão EQUIVALENTE já usado
// pro "vazio?" 9 dias antes). Slots via TweakDBID (não `gamedataEquipmentArea`, categoria
// diferente de `BwmsScanEmptySlots` acima) — mesmos slots já nomeados em
// `enablers/ArchiveXL/src/App/Extensions/Attachment/Extension.cpp`
// (HeadSlot/FaceSlot/TorsoSlot/ChestSlot/LegsSlot/FeetSlot) + Outfit/WeaponRight pra cobertura
// extra. Read-only (nenhum dos 2 nativos muta estado) — comparável em risco a `BwmsScanEmptySlots`.
public static func BwmsScanSpawningSlots(game: GameInstance) -> Void {
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[scanspawn] player indefinido");
    return;
  };
  let ts: ref<TransactionSystem> = GameInstance.GetTransactionSystem(game);
  if !IsDefined(ts) {
    Print("[scanspawn] GetTransactionSystem falhou");
    return;
  };
  let slots: array<TweakDBID>;
  ArrayPush(slots, t"AttachmentSlots.Head");
  ArrayPush(slots, t"AttachmentSlots.Eyes");
  ArrayPush(slots, t"AttachmentSlots.Torso");
  ArrayPush(slots, t"AttachmentSlots.Chest");
  ArrayPush(slots, t"AttachmentSlots.Legs");
  ArrayPush(slots, t"AttachmentSlots.Feet");
  ArrayPush(slots, t"AttachmentSlots.Outfit");
  ArrayPush(slots, t"AttachmentSlots.WeaponRight");
  let i: Int32 = 0;
  while i < ArraySize(slots) {
    let spawning: Bool = ts.IsSlotSpawningAnyItem(player, slots[i]);
    let empty: Bool = ts.IsSlotEmpty(player, slots[i]);
    Print("[scanspawn] slot#" + ToString(i) + " spawning=" + ToString(spawning) + " empty=" + ToString(empty));
    i += 1;
  };
}

public static func BwmsForceEquipRaw(game: GameInstance) -> Void {
  let want: Int32 = BwmsEquipState();
  if want <= 0 {
    Print("[equiprawonce] BwmsEquipState()<=0, nada a fazer (setar ~/.bwms-equip=1/2/3 antes)");
    return;
  };
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equiprawonce] player indefinido");
    return;
  };
  let id: TweakDBID = TDBID.None();
  if want == 1 { id = t"Items.GOG_DLC_Jacket_Legendary"; };
  if want == 2 { id = t"Items.Fixer_01_Set_TShirt"; };
  if want == 3 { id = t"Items.Coat_04_rich_02_Crafting"; };
  if !TDBID.IsValid(id) {
    Print("[equiprawonce] sel=" + ToString(want) + " sem item mapeado");
    return;
  };
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.FromTDBID(id);
  req.owner = player;
  req.addToInventory = true;
  req.slotIndex = -1;
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  if !IsDefined(es) {
    Print("[equiprawonce] EquipmentSystem.GetInstance falhou");
    return;
  };
  // 2026-08-03: traço passo-a-passo (achado: nem o log Rust de tramp_queue_request_raw nem o
  // Print de sucesso abaixo apareceram nos 2 crashes ao vivo — precisa isolar se o crash é
  // ANTES do dispatch pra BwmsQueueRequestRaw ou DENTRO dele).
  Print("[equiprawonce] pré-chamada: es e req prontos, chamando BwmsQueueRequestRaw agora");
  let ok: Bool = BwmsQueueRequestRaw(es, req);
  Print("[equiprawonce] pós-chamada: BwmsQueueRequestRaw retornou ok=" + ToString(ok) + " (sel=" + ToString(want) + ", via RAW, bypassa GetInvokable)");
}

// axl-garment-apply / axl-transmog-apply (2026-08-02, /goal): 3ª via, achado pelo agente que desmontou
// GetInvokable() em si (0x100339b2c — stub que SEMPRE retorna null; a causa real de "funciona vs
// crasha" é o `ctx` explícito no fast-path do caller, não um campo do descritor). Chama QueueRequest
// via `rtti::call_func` NOSSO (não bytecode redscript aninhado), com `ctx` = instância EquipmentSystem
// explícita — mesmo padrão já provado do `give` (TransactionSystem::GiveItem, ctx=tx explícito).
native func BwmsQueueRequestCtx(sys: ref<IScriptable>, req: ref<IScriptable>) -> Bool;

public static func BwmsForceEquipCtx(game: GameInstance) -> Void {
  let want: Int32 = BwmsEquipState();
  if want <= 0 {
    Print("[equipctxonce] BwmsEquipState()<=0, nada a fazer (setar ~/.bwms-equip=1/2/3 antes)");
    return;
  };
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equipctxonce] player indefinido");
    return;
  };
  let id: TweakDBID = TDBID.None();
  if want == 1 { id = t"Items.GOG_DLC_Jacket_Legendary"; };
  if want == 2 { id = t"Items.Fixer_01_Set_TShirt"; };
  if want == 3 { id = t"Items.Coat_04_rich_02_Crafting"; };
  if !TDBID.IsValid(id) {
    Print("[equipctxonce] sel=" + ToString(want) + " sem item mapeado");
    return;
  };
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.FromTDBID(id);
  req.owner = player;
  req.addToInventory = true;
  req.slotIndex = -1;
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  if !IsDefined(es) {
    Print("[equipctxonce] EquipmentSystem.GetInstance falhou");
    return;
  };
  let ok: Bool = BwmsQueueRequestCtx(es, req);
  Print("[equipctxonce] BwmsQueueRequestCtx ok=" + ToString(ok) + " (sel=" + ToString(want) + ", via call_func com ctx explícito)");
}

public class BwmsTppPoller extends DelayCallback {
  let m_game: GameInstance;
  let m_last: Int32;
  let m_stable: Int32;

  public func Call() -> Void {
    let want: Int32 = BwmsTppState();
    let player: ref<GameObject> = GameInstance.GetPlayerSystem(this.m_game).GetLocalPlayerControlledGameObject();
    if IsDefined(player) {
      if this.m_stable < 1000 { this.m_stable += 1; };
      if this.m_stable % 20 == 0 {
        BwmsPollerTick("tpp", this.m_stable);
      };
      if want == 1 {
        // MANTER SUAVE (2026-07-15): o ActivateTPPRepresentation é TRANSIENTE em free-roam (o jogo
        // reverte pra FPP; persiste ~25s). Re-disparar a cada 0.5s INTERROMPE o Activate antes de aplicar
        // (fica sempre FPP). Então: dispara no 0→1 E re-estabelece a cada ~15s (m_stable%30, tick=0.5s) —
        // suave, deixa cada Activate aplicar + persistir, re-fixa antes de reverter. Guarda m_stable>=4
        // (~2s): os pollers já nascem PÓS world-load (dylib chama BwmsBootFullbody só em gameplay via callg).
        if this.m_stable >= 4 && (this.m_last != 1 || this.m_stable % 10 == 0) {
          player.QueueEvent(new ActivateTPPRepresentationEvent());
          this.m_last = 1;
        };
      } else {
        if this.m_last == 1 {
          player.QueueEvent(new DeactivateTPPRepresentationEvent());
        };
        this.m_last = 0;
      };
      // TESTE 2026-07-15 (BwmsForceLook, opt-in via ~/.bwms-forcelook): sem HID pra olhar pra baixo
      // de verdade, força a rotação da câmera FPP pra baixo + aplica o offset máximo — só pra
      // provar/refutar visualmente se o torso (anexado via ActivateTPPRepresentation) aparece.
      // Reusa BwmsCamBack/BwmsCamZ (já existentes) como o offset a aplicar no pitch forçado.
      // Lido em LOCAL antes de comparar (2026-08-21): comparar o retorno de um native INLINE é a
      // forma quebrada já documentada (2026-07-15) e foi a causa do triplo impossível do `#18`.
      let fl: Int32 = BwmsForceLook();
      if fl == 1 {
        let pp: ref<PlayerPuppet> = player as PlayerPuppet;
        if IsDefined(pp) {
          let cam: ref<FPPCameraComponent> = pp.GetFPPCameraComponent();
          if IsDefined(cam) {
            let euler: EulerAngles;
            euler.Pitch = -60.0;
            euler.Yaw = 0.0;
            euler.Roll = 0.0;
            cam.SetLocalOrientation(EulerAngles.ToQuat(euler));
            let maxY: Float = Cast<Float>(BwmsCamBack()) / 100.0;
            let maxZ: Float = Cast<Float>(BwmsCamZ()) / 100.0;
            cam.SetLocalPosition(new Vector4(0.0, maxY, maxZ, 1.0));
          };
        };
      };
    };
    // FIX 2026-07-15 (mesmo achado do BwmsCamPoller): reagenda `this`, não um objeto novo.
    let ds: ref<DelaySystem> = GameInstance.GetDelaySystem(this.m_game);
    if IsDefined(ds) {
      ds.DelayCallback(this, 0.5);
    };
  }
}

// [FIX save-load 2026-07-15] NÃO wrapa NENHUMA classe do world-load (provado: wrapar PlayerPuppet OU
// BaseSubtitles corrompe o SystemsUpdater no world-load — o crash é da REGISTRAÇÃO do wrap linkando os
// pollers→ActivateTPPRepresentationEvent→TakeOverControlSystem, não da execução; stack 100% nativo).
// Em vez disso: função GLOBAL que o DYLIB chama (callg) quando detecta gameplay (pós-load). Pega o
// GameInstance via GetGameInstance() (não precisa de 'this' nem de wrapar classe). Chamável pelo canal:
// `callg BwmsBootFullbody`.
// ACHADO 2026-07-15: `GetGameInstance()` chamado de dentro desta função quando invocada via callg
// (call_func cru, ctx=null) devolvia uma GameInstance MORTA (GetDelaySystem/GetPlayerSystem
// retornavam undefined mesmo em gameplay real). FIX: o Rust agora passa a GameInstance JÁ
// RESOLVIDA (via PlayerPuppet.GetGame com ctx=player real) como ARGUMENTO — não conjuramos mais
// sozinhos aqui. Ver proofs/2026-07-15-callg-global-getgameinstance-dead-ACHADO.log.
// RECEITA REAL DO JB TPP MOD (2026-07-16): o corpo em pé de verdade NÃO vem só do
// ActivateTPPRepresentation — vem de ATIVAR o componente `tppCamera` nativo do player (é ele que
// bota o jogo no modo TPP real → corpo renderiza+anima em pé, sem IK comprimido). O mod faz:
// (1) ActivateTPPRepresentationEvent COM playerController setado; (2) FindComponentByName('tppCamera')
// .Activate(); (3) posiciona a câmera. Pro NOSSO caso (1ª pessoa olhando pra baixo): ativa o tppCamera
// mas posiciona PERTO DA CABEÇA (y≈0, z=1.7=altura da cabeça) em vez de atrás → visão FPP + corpo em pé.
// `@addMethod(PlayerPuppet)` (seguro, igual @addField já usado — NÃO é @wrapMethod, que crasha world-load).
// `FindComponentByName` é protected → só acessível de DENTRO de um método do próprio player (aqui).

@addMethod(PlayerPuppet)
public func BwmsActivateTppCam(atHead: Bool) -> Void {
  // NOTA: o campo `playerController` existe no runtime (o CET seta) mas NÃO está no stub redscript
  // do ActivateTPPRepresentationEvent (importonly, campo não exposto) → não dá pra setar aqui. A peça
  // central é o tppCamera; testar sem playerController primeiro. Se precisar, setar via reflexão Rust.
  this.QueueEvent(new ActivateTPPRepresentationEvent());
  // 2026-07-16: o `tppCamera` NÃO existe no player vanilla — quem o ADICIONA é o .archive do JB mod
  // (entity do player patchada: player_ma_fpp.ent/player_wa_fpp.ent + player_locomotion.animgraph),
  // agora instalado como archive/Mac/content/basegame_zzzz_jbtpp.archive (Path A, aprovado pelo
  // Perrotta). Com o archive carregado, o FindComponentByName deve ACHAR o tppCamera.
  // (Descartado: vehicleTPPCamera a pé — ativa mas o transform fica quebrado sem veículo, câmera
  // presa longe do corpo. Provado 2026-07-16.)
  let c1: ref<IComponent> = this.FindComponentByName(n"tppCamera");
  let cam: ref<CameraComponent> = c1 as CameraComponent;
  Print("[bwms-tppcam] tppCamera raw=" + ToString(IsDefined(c1)) + " asCameraComponent=" + ToString(IsDefined(cam)));
  // SONDA DECISIVA (2026-07-16): `WorldSpaceBlendCamera` está no BUFFER do .ent vanilla mas NÃO é
  // requisitado via RequestComponent no player.script. Se EXISTIR no player → componentes de buffer
  // instanciam sem request (culpado = save-spawn não re-lê o .ent). Se NÃO existir → só instancia o
  // que é requisitado (culpado = falta RequestComponent pro tppCamera). Escolhe a rota (a) vs (b).
  let probe: ref<IComponent> = this.FindComponentByName(n"WorldSpaceBlendCamera");
  Print("[bwms-tppcam] sonda WorldSpaceBlendCamera=" + ToString(IsDefined(probe)));
  if IsDefined(cam) {
    cam.Activate(0.2);
    if atHead {
      cam.SetLocalPosition(new Vector4(0.0, -0.3, 1.7, 1.0));
    };
    Print("[bwms-tppcam] tppCamera ATIVADO (atHead=" + ToString(atHead) + ")");
  };
  // OLHAR PRA BAIXO uma vez, aqui no one-shot (sem depender do poller que crasha), pra o screenshot
  // capturar a POSE do corpo COM o anim-graph do JB servido (reslink). Se as pernas estiverem
  // esticadas = o anim-graph do JB já resolve, e o tppCamera nem é necessário pro objetivo.
  let fcam: ref<FPPCameraComponent> = this.GetFPPCameraComponent();
  if IsDefined(fcam) {
    let e: EulerAngles;
    e.Pitch = -60.0; e.Yaw = 0.0; e.Roll = 0.0;
    fcam.SetLocalOrientation(EulerAngles.ToQuat(e));
    Print("[bwms-tppcam] olhar-pra-baixo forcado uma vez (pitch -60)");
  };
}

func BwmsBootFullbody(game: GameInstance) -> Void {
  let ds: ref<DelaySystem> = GameInstance.GetDelaySystem(game);
  Print("[bwms-fb-diag] ds=" + ToString(IsDefined(ds)));
  // ── TESTE DECISIVO 2026-07-15 (tpp_oneshot) ────────────────────────────────────────────────
  // Achado: o crash `SystemsUpdater::Node::LinkJob` (~1m57s, consistente) acontece com o TPP poller
  // agendado MESMO QUE o Call() do poller NUNCA dispare (0 ticks provados). Logo o crash é do próprio
  // ATO de agendar o DelayCallback (objeto script cai fora de escopo → engine toca ref pendente),
  // não do que o poller faz. Este caminho dispara o ActivateTPPRepresentation UMA VEZ, DIRETO, SEM
  // DelayCallback/poller — isola (a) o corpo em pé aparece? (b) sem o DelayCallback, o crash some?
  // Gate: ~/.bwms-modconfig.txt tpp_oneshot=1.
  // BUG ACHADO+CORRIGIDO 2026-07-15: ler BwmsConfigGet INLINE dentro do `Equals(...)` faz o marshalling
  // do retorno String falhar (Equals dá false mesmo com valor "1") — o MESMO bug que eu já tinha
  // corrigido nos pollers (ler em local primeiro), reintroduzido sem querer aqui. Provado: o boot
  // leu tpp_oneshot='1' (log do native) mas o bloco NÃO rodou (nenhum Print, nenhum screenshot).
  let cfgOneshot: String = BwmsConfigGet("tpp_oneshot");
  let cfgNativeCam: String = BwmsConfigGet("tppcam_native");
  if Equals(cfgOneshot, "1") {
    let p1: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
    Print("[bwms-oneshot] player IsDefined=" + ToString(IsDefined(p1)));
    if IsDefined(p1) {
      if Equals(cfgNativeCam, "1") {
        // RECEITA JB TPP: ativa o tppCamera nativo (corpo em pé real), câmera na cabeça (1ª pessoa).
        let pp: ref<PlayerPuppet> = p1 as PlayerPuppet;
        if IsDefined(pp) {
          pp.BwmsActivateTppCam(true);
          Print("[bwms-oneshot] via BwmsActivateTppCam (tppCamera nativo, atHead)");
        };
      } else {
        p1.QueueEvent(new ActivateTPPRepresentationEvent());
        Print("[bwms-oneshot] ActivateTPPRepresentationEvent enfileirado (direto, sem poller/DelayCallback)");
      };
    };
  };
  // BISECT 2026-07-15: o crash `SystemsUpdater::Node::LinkJob_NoFence` foi isolado na infra do
  // poller/auto-trigger em si (sobrevive 240s sem NENHUM poller criado), mas ainda não sabemos QUAL
  // dos 3 é a causa. Gateia cada um atrás de `~/.bwms-modconfig.txt` (poller_tpp/poller_equip/
  // poller_inv = "1") pra testar 1 por vez sem precisar recompilar o dylib entre tentativas.
  // FIX DEFENSIVO (2026-07-15, mesma continuação, mais tarde): observado ao vivo que com
  // poller_tpp='1'/poller_equip=''/poller_inv='' CONFIRMADOS no log, `BwmsEquipPoller` ainda assim
  // rodou (3000+ ciclos) enquanto `BwmsTppPoller` NUNCA tickou (nem 1x, mesmo com tppcam=0, sem
  // tentar ActivateTPPRepresentation) — ou seja, a criação/agendamento do próprio poller falha, não
  // é o Activate que mata. Suspeito: reler `BwmsConfigGet` 3x seguidas (mesmo native, args String)
  // direto dentro de 3 `if Equals(...)` pode ter uma interação de marshalling entre chamadas
  // consecutivas. Fix defensivo: ler os 3 valores em variáveis locais PRIMEIRO, comparar depois.
  let cfgTpp: String = BwmsConfigGet("poller_tpp");
  let cfgEquip: String = BwmsConfigGet("poller_equip");
  let cfgInv: String = BwmsConfigGet("poller_inv");
  if IsDefined(ds) {
    if Equals(cfgTpp, "1") {
      let cam: ref<BwmsTppPoller> = new BwmsTppPoller();
      Print("[bwms-tpp-diag] new BwmsTppPoller() IsDefined=" + ToString(IsDefined(cam)));
      cam.m_game = game; cam.m_last = 0;
      ds.DelayCallback(cam, 0.5);
      Print("[bwms-tpp-diag] DelayCallback(cam,0.5) chamado");
    };
    // BwmsCamPoller (offset adaptativo do FPPCameraComponent) REMOVIDO do boot: ele força a posição
    // da câmera FPP a cada 0.03s e BRIGA com a câmera 3ª-pessoa que o ActivateTPPRepresentation põe,
    // revertendo o corpo pra FPP. Sem ele, o Activate manda na câmera. (Teste 2026-07-15.)
    if Equals(cfgEquip, "1") {
      let eq: ref<BwmsEquipPoller> = new BwmsEquipPoller();   // Skill 2: equipar por código
      eq.m_game = game; eq.m_last = 0; eq.m_lastCheck = 0;
      ds.DelayCallback(eq, 0.6);
    };
    if Equals(cfgInv, "1") {
      let inv: ref<BwmsInvPoller> = new BwmsInvPoller();      // Skill 1b: preview do inventário
      inv.m_game = game; inv.m_last = 0;
      ds.DelayCallback(inv, 0.7);
    };
  };
  // redscript-mod-persistence: restaura o God Mode se a config externa (~/.bwms-modconfig.txt,
  // FORA do save) diz que estava ligado — completa o round-trip que faltava (a primitiva
  // BwmsConfigGet/Set já era provada desde 2026-07-13, mas nenhum cheat real a usava ainda).
  // Roda aqui (BwmsBootFullbody) porque este ponto já tem GameInstance+player resolvidos de
  // forma confiável (fix do achado GetGameInstance-morto, mesma sessão).
  // (mesmo fix inline→local do one-shot acima: o godmode-restore também lia BwmsConfigGet inline,
  // então provavelmente NUNCA restaurou de fato — bug latente, corrigido aqui de tabela.)
  let cfgGod: String = BwmsConfigGet("godmode");
  if Equals(cfgGod, "1") {
    let pl: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
    let pp: ref<PlayerPuppet> = pl as PlayerPuppet;
    if IsDefined(pp) {
      GameInstance.GetGodModeSystem(game).AddGodMode(pp.GetEntityID(), gameGodModeType.Invulnerable, n"BWMS");
    };
  };
}

// BwmsTppRefire(game) — RE-DISPARO do full-body dirigido pelo TICK LOOP DO RUST (estável), NÃO por
// DelayCallback de redscript (que crasha: o objeto script cai de escopo no contexto callg-anormal e
// o motor toca a ref pendente ~1m57s depois → SystemsUpdater::Node::LinkJob). Cada chamada é um
// ONE-SHOT independente: pega o player, e se o toggle da câmera-TPP (~/.bwms-tppcam) está ligado,
// enfileira UM ActivateTPPRepresentationEvent (o efeito é transiente em free-roam, ~25s, então o
// Rust re-chama isto a cada ~10s pra manter o corpo). Zero objeto de vida-longa, zero DelayCallback.
// O dylib resolve esta global (get_function) e chama via call_func passando a GameInstance real —
// mesmo caminho já provado de BwmsBootFullbody.
func BwmsTppRefire(game: GameInstance) -> Void {
  // Lido em LOCAL antes de comparar (2026-08-21) — ver nota acima.
  let tppSt: Int32 = BwmsTppState();
  if tppSt != 1 { return; };
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if IsDefined(player) {
    // PLANO B (2026-07-16, gated `no_reactivate=1`): hipótese = segurar `isTPP`=true continuamente
    // (BwmsLegsHold, ~1.5s) já MANTÉM o corpo anexado, tornando o re-disparo do ActivateTPP
    // DESNECESSÁRIO. Como o re-disparo é a CAUSA da oscilação da pose (reseta pra encolhido), pular
    // ele deve dar pernas em pé ESTÁVEIS. Se o corpo sumir (destacar) sem o re-disparo, a hipótese
    // cai e volta pro plano A (refire + hold rápido). A/B sem recompilar (modconfig).
    let noReact: String = BwmsConfigGet("no_reactivate");
    if NotEquals(noReact, "1") {
      player.QueueEvent(new ActivateTPPRepresentationEvent());
    };
    // StandEnter uma vez por refire (evento de transição — dispara a entrada no estado em pé).
    let legs: String = BwmsConfigGet("legs");
    if Equals(legs, "1") {
      AnimationControllerComponent.PushEvent(player, n"StandEnter");
    };
  };
}

// BwmsLegsHold(game) — SEGURA as pernas em pé (2026-07-16). Achado: setar `fullbody`/`isTPP` uma vez
// a cada ~10s (junto do refire) faz a pose OSCILAR — o re-disparo do ActivateTPPRepresentation reseta
// pra pose encolhida e briga. Fix: aplicar as vars booleanas do anim-graph (`fullbody`/`isTPP`) numa
// cadência RÁPIDA (o dylib chama isto a cada ~90 ticks, ~1.5s), pra segurar a pose em pé ENTRE os
// resets do ActivateTPP. Só inputs contínuos (sem evento/ActivateTPP), crash-free (QueueEvent).
func BwmsLegsHold(game: GameInstance) -> Void {
  let legs: String = BwmsConfigGet("legs");
  if NotEquals(legs, "1") { return; };
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if IsDefined(player) {
    AnimationControllerComponent.SetInputBool(player, n"fullbody", true);
    AnimationControllerComponent.SetInputBool(player, n"isTPP", true);
    // TESTE 2026-07-16 (achado por RE do CR2W do player_locomotion.animgraph — dump da
    // animAnimVariableContainer, chunk #3618): `crouch` é a 1ª de só 58 float-vars do graph —
    // candidata forte a controlar a transição de estado fpp_crouch_idle <-> fpp_idle_stand. Gate
    // `stand=1` (novo, independente de `legs`) — testar isolado antes de assumir que resolve.
    let stand: String = BwmsConfigGet("stand");
    if Equals(stand, "1") {
      AnimationControllerComponent.SetInputFloat(player, n"crouch", 0.0);
    };
    // MODO JOGÁVEL (foco no que o Perrotta pediu): SEM forçar o olhar (o jogador controla a câmera e
    // joga normal); só AFASTA a câmera pra trás/cima pra ver o corpo (3ª-pessoa-ish). Gate `look=1`.
    // Câmera ajustável ao vivo por BwmsCamBack (trás, Y-, CM) / BwmsCamZ (cima, Z, CM) — sem recompilar.
    let look: String = BwmsConfigGet("look");
    if Equals(look, "1") {
      let pp: ref<PlayerPuppet> = player as PlayerPuppet;
      if IsDefined(pp) {
        let fcam: ref<FPPCameraComponent> = pp.GetFPPCameraComponent();
        if IsDefined(fcam) {
          let back: Float = Cast<Float>(BwmsCamBack()) / 100.0;
          let up: Float = Cast<Float>(BwmsCamZ()) / 100.0;
          fcam.SetLocalPosition(new Vector4(0.0, back, up, 1.0));
        };
      };
    };
  };
}

// `BwmsCamTiltOnce(game)` — DIAGNÓSTICO 2026-07-17: isola se o SetLocalOrientation (a) nunca aplica
// de vez, ou (b) aplica e é revertido pela câmera nativa no frame seguinte. Recebe `game` já resolvido
// (padrão BwmsBootFullbody) e faz a inclinação DIRETO (sem poller/delay) — chamada e screenshot devem
// acontecer no MESMO instante (a chamada é síncrona; o screenshot é tirado logo em seguida pelo lado
// Rust, sem esperar nenhum tick).
public func BwmsCamTiltOnce(game: GameInstance) -> Void {
  let pl: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  let pp: ref<PlayerPuppet> = pl as PlayerPuppet;
  if !IsDefined(pp) {
    Print("[camtilt] sem player");
    return;
  };
  let cam: ref<FPPCameraComponent> = pp.GetFPPCameraComponent();
  if !IsDefined(cam) {
    Print("[camtilt] GetFPPCameraComponent() falhou");
    return;
  };
  let euler: EulerAngles;
  euler.Pitch = -60.0;
  euler.Yaw = 0.0;
  euler.Roll = 0.0;
  cam.SetLocalOrientation(EulerAngles.ToQuat(euler));
  cam.SetLocalPosition(new Vector4(0.0, 0.0, 0.0, 1.0));
  // Lê de volta IMEDIATAMENTE (mesmo frame do script) — confirma se a escrita ficou visível já aqui.
  let q = cam.GetLocalOrientation();
  let e2: EulerAngles = Quaternion.ToEulerAngles(q);
  Print("[camtilt] SetLocalOrientation(pitch=-60) aplicado; readback imediato pitch=" + ToString(e2.Pitch));
}

// `BwmsFullbodyTiltTest(game)` — 2026-07-17: torso + tilt NUMA CHAMADA SÓ (menos round-trips de canal
// = menos superfície pra crash). Ângulo MODERADO (-35°, "olhar pro peito"/posição casaco, não -60°
// que só mostra os pés) — pra ver o TORSO anexado pelo ActivateTPPRepresentation, não só o chão.
public func BwmsFullbodyTiltTest(game: GameInstance) -> Void {
  let pl: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  let pp: ref<PlayerPuppet> = pl as PlayerPuppet;
  if !IsDefined(pp) {
    Print("[fbtilt] sem player");
    return;
  };
  pl.QueueEvent(new ActivateTPPRepresentationEvent());
  let cam: ref<FPPCameraComponent> = pp.GetFPPCameraComponent();
  if !IsDefined(cam) {
    Print("[fbtilt] torso enfileirado; GetFPPCameraComponent() falhou");
    return;
  };
  let euler: EulerAngles;
  euler.Pitch = -35.0;
  euler.Yaw = 0.0;
  euler.Roll = 0.0;
  cam.SetLocalOrientation(EulerAngles.ToQuat(euler));
  Print("[fbtilt] torso enfileirado (ActivateTPPRepresentationEvent) + câmera pitch=-35 aplicado");
}

// `BwmsLegsTiltTest(game)` — 2026-07-17: teste DEFINITIVO do problema REAL (pernas dobradas vs em pé,
// esclarecido pelo Perrotta — NÃO é câmera). Combina numa chamada só: torso (ActivateTPP) + pernas
// (fullbody/isTPP=true + crouch=0.0, a receita do BwmsLegsHold/2026-07-16) + câmera num ângulo MAIS
// ABERTO (-22°, "olhar mais reto" que enquadra torso+pernas, não só o peito) — pra ver se as pernas
// esticam OU seguem na pose casaco/agachada.
public func BwmsLegsTiltTest(game: GameInstance) -> Void {
  let pl: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  let pp: ref<PlayerPuppet> = pl as PlayerPuppet;
  if !IsDefined(pp) {
    Print("[legstilt] sem player");
    return;
  };
  // torso
  pl.QueueEvent(new ActivateTPPRepresentationEvent());
  // pernas: mesma receita do BwmsLegsHold (fullbody/isTPP contínuos + crouch=0.0), aplicada 1x aqui
  // (o efeito de "contínuo" viria do refire, mas pra este teste único já mostra se a var MEXE a pose).
  AnimationControllerComponent.SetInputBool(pl, n"fullbody", true);
  AnimationControllerComponent.SetInputBool(pl, n"isTPP", true);
  AnimationControllerComponent.SetInputFloat(pl, n"crouch", 0.0);
  AnimationControllerComponent.PushEvent(pl, n"StandEnter");
  // câmera: ângulo mais aberto que o -35 do teste anterior, pra enquadrar torso+pernas (não só peito).
  let cam: ref<FPPCameraComponent> = pp.GetFPPCameraComponent();
  if IsDefined(cam) {
    let euler: EulerAngles;
    euler.Pitch = -22.0;
    euler.Yaw = 0.0;
    euler.Roll = 0.0;
    cam.SetLocalOrientation(EulerAngles.ToQuat(euler));
  };
  Print("[legstilt] torso+pernas(fullbody/isTPP/crouch=0/StandEnter)+câmera(-22) aplicados numa chamada");
}

// `BwmsGetSceneTier(game)` — 2026-07-18: achado do coordenador — o teste de `legstilt` anterior rodou
// contra um save cujo autocontinue pousou NUMA CENA/ANIMAÇÃO ROTEIRIZADA (V sentado, jornal na mão,
// cenário de barco), não em free-roam normal — confound real, não responde "as pernas esticam?".
// `PlayerPuppet.GetSceneTier` (static, player.script:4569) lê `PlayerStateMachineBlackboard.HighLevel`
// (`gamePSMHighLevel`, orphans.script:6676): 0=Default (free-roam de verdade), 1-5=SceneTier1..5
// (dialogo/cutscene/animação roteirizada), 6=Swimming. -1 = sem player (erro). Sinal CONFIÁVEL pra
// "V está andando/parado sob controle normal" vs "V está numa cena que o jogo está dirigindo" —
// nem phase5 nem "save carregou" garantem isso (phase5 só garante que o MUNDO carregou, não que o
// player já tem controle livre). Checar isto == 0 ANTES de disparar `legstilt` de novo.
public func BwmsGetSceneTier(game: GameInstance) -> Int32 {
  let pl: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  let pp: ref<PlayerPuppet> = pl as PlayerPuppet;
  if !IsDefined(pp) {
    Print("[scenetier] sem player");
    return -1;
  };
  let tier: Int32 = PlayerPuppet.GetSceneTier(pp);
  Print(s"[scenetier] tier=\(tier) (0=free-roam real, 1-5=cena/animação roteirizada, 6=nadando)");
  return tier;
}

// axl-garment-apply / axl-transmog-apply (2026-08-03, /goal): 4ª via — evita os 2 bugs reais já
// diagnosticados nesta madrugada (HISTORICO.md cont.17/19). Constrói es+req NUM SÓ lugar (zero 2ª
// call_func TOP-LEVEL) e captura os ponteiros via 2 natives ANINHADAS de 1-arg-handle cada
// (BwmsCaptureEs/Req — mesmo shape seguro de BwmsCallMethod), em vez de passar 2 handles numa
// chamada só (o que crashava em equiprawonce/equipctxonce). Retorna Void — nem essa função em si
// dispara o bug de "2ª call_func retornando ref<T>". Rust lê os atomics DEPOIS que este call_func
// único retorna, e faz o transmute pro endereço cru inteiramente do lado Rust.
native func BwmsCaptureEs(es: ref<IScriptable>) -> Void;
native func BwmsCaptureReq(req: ref<IScriptable>) -> Void;

public static func BwmsPrepAndCapture(game: GameInstance) -> Void {
  Print("[equiprawv3] ENTROU em BwmsPrepAndCapture (1ª linha, antes de qualquer coisa)");
  let want: Int32 = BwmsEquipState();
  if want <= 0 {
    Print("[equiprawv3] BwmsEquipState()<=0, nada a fazer (setar ~/.bwms-equip=1/2/3 antes)");
    return;
  };
  let player: ref<GameObject> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject();
  if !IsDefined(player) {
    Print("[equiprawv3] player indefinido");
    return;
  };
  let id: TweakDBID = TDBID.None();
  if want == 1 { id = t"Items.GOG_DLC_Jacket_Legendary"; };
  if want == 2 { id = t"Items.Fixer_01_Set_TShirt"; };
  if want == 3 { id = t"Items.Coat_04_rich_02_Crafting"; };
  if !TDBID.IsValid(id) {
    Print("[equiprawv3] sel=" + ToString(want) + " sem item mapeado");
    return;
  };
  let req: ref<EquipRequest> = new EquipRequest();
  req.itemID = ItemID.FromTDBID(id);
  req.owner = player;
  req.addToInventory = true;
  req.slotIndex = -1;
  let es: ref<EquipmentSystem> = EquipmentSystem.GetInstance(player);
  if !IsDefined(es) {
    Print("[equiprawv3] EquipmentSystem.GetInstance falhou");
    return;
  };
  Print("[equiprawv3] pré-captura: es e req prontos, capturando es agora");
  BwmsCaptureEs(es);
  Print("[equiprawv3] es capturado, capturando req agora");
  BwmsCaptureReq(req);
  Print("[equiprawv3] req capturado, retornando (Void) — Rust faz o transmute a seguir");
}
