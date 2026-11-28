// blackwall-mods.reds — o botão MODS e a página de cheats na lista do menu de pausa, e a entrada do V no jogo (`BwmsAoEntrarNoJogo`).
// A página reusa Clear/AddMenuItem/Refresh herdados de gameuiMenuItemListGameController (texto limpo), sem Lua e sem hook de runtime;
// só no menu de PAUSA (precisa de player vivo).
//
// Modo Imortal le o ESTADO REAL do God Mode via GodModeSystem.HasGodMode (sem campo proprio
// que reseta). Precedente vanilla: cyberpunk/UI/Player/healthbar.script:683.
//
// RAM / NPCs / Municao seguem a mesma disciplina do resto do projeto: cada um usa o mecanismo
// que o PROPRIO jogo usa, e cada escolha aqui saiu de medicao em jogo, nao de leitura de nome.
// O que foi descartado, com o porque, esta anotado em cada bloco — pra ninguem refazer o caminho.

@addField(PauseMenuGameController) let m_bwInModsPage: Bool;

// Ligado/desligado de cada cheat continuo fica na config externa (`cheat_npc`, `cheat_municao`, `cheat_ram`, `cheat_pulo`, `cheat_sande`, `cheat_isinlan`), como o `godmode`: fora do save e
// fora do PlayerPuppet, que nasce de novo a cada carga. Os efeitos do NPCs vao pro save; com o estado num campo do V, a carga mostrava
// DESLIGADO com os NPCs ainda amigos. O `BwmsAoEntrarNoJogo` religa pela config. No PlayerPuppet fica so o que e da sessao.
@addField(PlayerPuppet) let m_bwVisMod: ref<gameStatModifierData>;
@addField(PlayerPuppet) let m_bwMagMod: ref<gameStatModifierData>;
@addField(PlayerPuppet) let m_bwMagWeapon: wref<WeaponObject>;
@addField(PlayerPuppet) let m_bwTicking: Bool;
@addField(PlayerPuppet) let m_bwPuloMod: ref<gameStatModifierData>;
@addField(PlayerPuppet) let m_bwPuloLigado: Bool;
@addField(PlayerPuppet) let m_bwSandeAtivo: Bool;
@addField(PlayerPuppet) let m_bwHarita: wref<WorldMapMenuGameController>;

public func BwmsCheatLigado(chave: String) -> Bool {
  return Equals(BwmsConfigGet(chave), "1");
}

public func BwmsCheatGravar(chave: String, on: Bool) -> Void {
  BwmsConfigSet(chave, on ? "1" : "0");
}

// Callback do tick. Classe dedicada em vez do despacho por nome do BwmsDelay: aquele caminho
// depende de uma nativa do BWMS, e este nao precisa de nenhuma — so API do jogo.
public class BwmsCheatTick extends DelayCallback {
  public let Player: wref<PlayerPuppet>;
  public func Call() -> Void {
    if IsDefined(this.Player) {
      this.Player.BWMSCheatTick();
    };
  }
}

// Reagenda enquanto algum cheat continuo estiver ligado. `m_bwTicking` evita dois tickers
// vivos ao mesmo tempo se o jogador ligar RAM e Municao em seguida.
//
// 2 segundos, nao um: o custo aqui nao e a conta, e o `GiveItem` — cada um levanta os eventos de
// inventario do jogo. Nem a RAM nem a reserva de municao caem rapido o bastante pra justificar
// mais que isso.
@addMethod(PlayerPuppet)
public func BWMSCheatSchedule() -> Void {
  if this.m_bwTicking { return; };
  if !BwmsCheatLigado("cheat_ram") && !BwmsCheatLigado("cheat_municao") { return; };
  this.m_bwTicking = true;
  let cb: ref<BwmsCheatTick> = new BwmsCheatTick();
  cb.Player = this;
  GameInstance.GetDelaySystem(this.GetGame()).DelayCallback(cb, 2.00, false);
}

@addMethod(PlayerPuppet)
public func BWMSCheatTick() -> Void {
  this.m_bwTicking = false;
  // RAM ilimitada: o jogo nao expoe "custo zero", o que existe e o POOL. Entao "ilimitado" aqui
  // e o pool Memory sendo enchido de volta — que e o que o jogador percebe.
  if BwmsCheatLigado("cheat_ram") {
    this.BWMSFillRam();
  };
  // Municao: o pente grande mata a recarga, mas a RESERVA continua caindo porque e dela que a
  // recarga puxa. Repor a reserva e a outra metade — sem ela o cheat parece nao funcionar.
  if BwmsCheatLigado("cheat_municao") {
    this.BWMSTopUpAmmo(t"Ammo.HandgunAmmo");
    this.BWMSTopUpAmmo(t"Ammo.RifleAmmo");
    this.BWMSTopUpAmmo(t"Ammo.ShotgunAmmo");
    this.BWMSTopUpAmmo(t"Ammo.SniperRifleAmmo");
    this.BWMSApplyMagazine();
  };
  this.BWMSCheatSchedule();
}

@addMethod(PlayerPuppet)
public func BWMSFillRam() -> Void {
  GameInstance.GetStatPoolsSystem(this.GetGame()).RequestSettingStatPoolValue(
    Cast<StatsObjectID>(this.GetEntityID()), gamedataStatPoolType.Memory, 100.00, this, true);
}

// So repoe o que FALTA. Dar municao de dois em dois segundos sem olhar quanto ja existe enche a
// tela de "ALINDI: ..." — cada GiveItem levanta a notificacao de item recebido do jogo, mesmo
// quando a reserva ja esta cheia. Perguntar a quantidade antes custa uma chamada e cala o resto.
@addMethod(PlayerPuppet)
public func BWMSTopUpAmmo(ammo: TweakDBID) -> Void {
  let ts: ref<TransactionSystem> = GameInstance.GetTransactionSystem(this.GetGame());
  let id: ItemID = ItemID.FromTDBID(ammo);
  // Limiar alto e reposicao grande de proposito: assim a notificacao aparece raramente, e quando
  // aparece corresponde a municao de verdade gasta.
  if ts.GetItemQuantity(this, id) < 100 {
    ts.GiveItem(this, id, 500);
  };
}

// O modificador vive na ARMA, entao trocar de arma o deixaria para tras. Remover e reaplicar a
// cada tick e mais simples e sempre correto — mais barato que guardar identidade de arma.
@addMethod(PlayerPuppet)
public func BWMSApplyMagazine() -> Void {
  this.BWMSClearMagazine();
  let w: ref<WeaponObject> = GameObject.GetActiveWeapon(this);
  if !IsDefined(w) { return; };
  this.m_bwMagMod = RPGManager.CreateStatModifier(
    gamedataStatType.MagazineCapacity, gameStatModifierType.Additive, 999.00);
  GameInstance.GetStatsSystem(this.GetGame())
    .AddModifier(Cast<StatsObjectID>(w.GetEntityID()), this.m_bwMagMod);
  this.m_bwMagWeapon = w;
}

@addMethod(PlayerPuppet)
public func BWMSClearMagazine() -> Void {
  if IsDefined(this.m_bwMagMod) && IsDefined(this.m_bwMagWeapon) {
    GameInstance.GetStatsSystem(this.GetGame())
      .RemoveModifier(Cast<StatsObjectID>(this.m_bwMagWeapon.GetEntityID()), this.m_bwMagMod);
  };
  this.m_bwMagMod = null;
  this.m_bwMagWeapon = null;
}

// "Os NPCs te deixam em paz". Limite honesto, porque o nome promete mais: NAO e invisibilidade —
// os NPCs continuam ENXERGANDO; o que muda e que param de atacar.
//
// Estes tres records vieram de medicao em jogo. O que foi descartado, e por que:
//   BaseStatusEffect.Cloaked  — ENTRA no jogador (HasStatusEffect responde SIM) e a percepcao
//                               nao muda: e o efeito do lado dos NPCs (Oda/MaxTac).
//   OpticalCamoIsActive       — aceita a escrita (0 -> 1 confirmado) e nao liga nada: e um stat
//                               que o jogo LE, nao um que obedece.
//
// Ligar so aplica o efeito que ainda nao esta no V: o save traz os efeitos, e o religar da carga somaria uma pilha a cada save carregado.
// Desligar tira a pilha inteira pelo mesmo motivo.
@addMethod(PlayerPuppet)
public func BWMSSetCloak(on: Bool) -> Void {
  let game: GameInstance = this.GetGame();
  let ses: ref<StatusEffectSystem> = GameInstance.GetStatusEffectSystem(game);
  let ss: ref<StatsSystem> = GameInstance.GetStatsSystem(game);
  let id: EntityID = this.GetEntityID();
  let soid: StatsObjectID = Cast<StatsObjectID>(id);
  let efeitos: array<TweakDBID> = [t"BaseStatusEffect.BlockTargetingPlayer", t"BaseStatusEffect.DontShootAtMe", t"BaseStatusEffect.SetFriendly"];
  if on {
    for e in efeitos {
      if !ses.HasStatusEffect(id, e) {
        ses.ApplyStatusEffect(id, e);
      };
    };
    if !IsDefined(this.m_bwVisMod) {
      this.m_bwVisMod = RPGManager.CreateStatModifier(
        gamedataStatType.Visibility, gameStatModifierType.Additive, -1000.00);
      ss.AddModifier(soid, this.m_bwVisMod);
    };
  } else {
    for e in efeitos {
      ses.RemoveStatusEffect(id, e, 99u);
    };
    // Heranca: as tentativas anteriores aplicavam `Cloaked`, que entra de verdade e vai junto pro
    // SAVE, deixando o personagem invisivel ao carregar. Desligar tem que limpa-lo.
    ses.RemoveStatusEffect(id, t"BaseStatusEffect.Cloaked", 99u);
    if IsDefined(this.m_bwVisMod) {
      ss.RemoveModifier(soid, this.m_bwVisMod);
      this.m_bwVisMod = null;
    };
  };
  BwmsCheatGravar("cheat_npc", on);
}

// Pulo sem limite no ar. O `DoubleJumpDecisions` conta os pulos no ar em `currentNumberOfJumps` e para no `numberOfMultiJumps` (1 com o
// Reinforced Tendons); zerar o contador antes da decisao libera o proximo. Sem o implante a decisao recusa antes, pelo `HasDoubleJump`, entao
// o cheat poe o flag tambem. O `EnterCondition` roda a cada quadro no ar: le o campo do V, nao a config, que e chamada nativa.
@addMethod(PlayerPuppet)
public func BWMSSetMultiJump(on: Bool) -> Void {
  let ss: ref<StatsSystem> = GameInstance.GetStatsSystem(this.GetGame());
  let soid: StatsObjectID = Cast<StatsObjectID>(this.GetEntityID());
  if on && !IsDefined(this.m_bwPuloMod) {
    this.m_bwPuloMod = RPGManager.CreateStatModifier(gamedataStatType.HasDoubleJump, gameStatModifierType.Additive, 1.00);
    ss.AddModifier(soid, this.m_bwPuloMod);
  };
  if !on && IsDefined(this.m_bwPuloMod) {
    ss.RemoveModifier(soid, this.m_bwPuloMod);
    this.m_bwPuloMod = null;
  };
  this.m_bwPuloLigado = on;
  BwmsCheatGravar("cheat_pulo", on);
}

@wrapMethod(DoubleJumpDecisions)
protected const func EnterCondition(const stateContext: ref<StateContext>, const scriptInterface: ref<StateGameScriptInterface>) -> Bool {
  let pp: ref<PlayerPuppet> = scriptInterface.executionOwner as PlayerPuppet;
  if IsDefined(pp) && pp.m_bwPuloLigado {
    stateContext.SetPermanentIntParameter(n"currentNumberOfJumps", 0, true);
  };
  return wrappedMethod(stateContext, scriptInterface);
}

// Sandevistan: o mundo anda na fracao `sande_escala` (0.001, 10, 25, 50 ou 75 por cento; 25 sem a chave) e o V na velocidade normal. O menu MODS
// liga a tecla (`cheat_sande`); Option+Z vem pelo Rust (`tempo::tecla_de_atalho`), que chama `BwmsSandevistanTecla` na thread do
// jogo. O Z sozinho brigava com o radio e o desmontar do inventario. Ativo ou nao e da sessao: a dilatacao do TimeSystem nao vai pro save.
public func BwmsSandevistanEscala() -> String {
  let v: String = BwmsConfigGet("sande_escala");
  if Equals(v, "0.001") || Equals(v, "10") || Equals(v, "25") || Equals(v, "50") || Equals(v, "75") { return v; };
  return "25";
}

@addMethod(PlayerPuppet)
public func BWMSSandevistan(on: Bool) -> Void {
  let ts: ref<TimeSystem> = GameInstance.GetTimeSystem(this.GetGame());
  if on {
    ts.SetTimeDilation(n"BwmsSandevistan", StringToFloat(BwmsSandevistanEscala()) / 100.00);
    ts.SetIgnoreTimeDilationOnLocalPlayerZero(true);
  } else {
    ts.UnsetTimeDilation(n"BwmsSandevistan");
    ts.SetIgnoreTimeDilationOnLocalPlayerZero(false);
  };
  this.m_bwSandeAtivo = on;
}

public func BwmsSandevistanTecla(game: GameInstance) -> Void {
  let pp: ref<PlayerPuppet> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject() as PlayerPuppet;
  if !IsDefined(pp) || !BwmsCheatLigado("cheat_sande") { return; };
  pp.BWMSSandevistan(!pp.m_bwSandeAtivo);
}

// Teleporte: com o mapa aberto, Option+X leva o V ao marcador sob o cursor (o seu, de missao, de viagem rapida). O mapa se anota no V ao
// abrir, pra `BwmsIsinlanTecla` (chamada pelo Rust, `tempo::tecla_de_atalho`) achar o `selectedMappin`. Um teleporte so deixava o V cair: o
// chao do destino ainda nao estava carregado. O `BwmsIsinlanTutucu` repete o teleporte a cada 0,1 s por 6 s de jogo, ate o mundo dali
// carregar. O primeiro tambem vai pelo DelaySystem, que para com o menu aberto: teleportar com o mapa aberto deixava a cena 3D do mapa na
// tela depois de fechar (preto com nuvens, ate abrir e fechar o ESC). A posicao vai em `isinlan_son` na config.
@wrapMethod(WorldMapMenuGameController)
protected cb func OnInitialize() -> Bool {
  let r: Bool = wrappedMethod();
  let pp: ref<PlayerPuppet> = this.GetPlayerControlledObject() as PlayerPuppet;
  if IsDefined(pp) { pp.m_bwHarita = this; };
  return r;
}

public class BwmsIsinlanTutucu extends DelayCallback {
  public let Player: wref<PlayerPuppet>;
  public let Hedef: Vector4;
  public let Kalan: Int32;
  public func Call() -> Void {
    let pp: ref<PlayerPuppet> = this.Player;
    if !IsDefined(pp) { return; };
    GameInstance.GetTeleportationFacility(pp.GetGame()).Teleport(pp, this.Hedef, pp.GetWorldOrientation().ToEulerAngles());
    this.Kalan -= 1;
    if this.Kalan > 0 {
      GameInstance.GetDelaySystem(pp.GetGame()).DelayCallback(this, 0.10, false);
    };
  }
}

public func BwmsIsinlanTecla(game: GameInstance) -> Void {
  let pp: ref<PlayerPuppet> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject() as PlayerPuppet;
  if !IsDefined(pp) || !BwmsCheatLigado("cheat_isinlan") { return; };
  let harita: ref<WorldMapMenuGameController> = pp.m_bwHarita;
  if !IsDefined(harita) || !IsDefined(harita.selectedMappin) { return; };
  let m: wref<IMappin> = harita.selectedMappin.GetMappin();
  if !IsDefined(m) { return; };
  let pos: Vector4 = m.GetWorldPosition();
  BwmsConfigSet("isinlan_son", FloatToString(pos.X) + " " + FloatToString(pos.Y) + " " + FloatToString(pos.Z));
  pos.Z += 1.00;
  let t: ref<BwmsIsinlanTutucu> = new BwmsIsinlanTutucu();
  t.Player = pp;
  t.Hedef = pos;
  t.Kalan = 60;
  GameInstance.GetDelaySystem(game).DelayCallback(t, 0.30, false);
}

// Desliga o God Mode do BWMS: a fonte atual e as antigas (`BlackwallMods` do botão MODS e `Console` do terminal até a 0.1.5). O jogo grava o
// GodModeSystem no save, então um save antigo traz essas entradas e tirar só a `BWMS` deixa o V invulnerável. A chave `godmode` da config externa
// zera junto: o `BwmsAoEntrarNoJogo` religa por ela na carga, e o OFF de qualquer caminho tem de valer no próximo save carregado.
//
// Os DOIS tipos, nao so um: `Invulnerable` e "nao toma dano" e `Immortal` e "nao morre". Ligar so um deixa um buraco de semantica.
public func BwmsGodModeDesligar(sys: ref<GodModeSystem>, id: EntityID) -> Void {
  let fontes: array<CName> = [n"BWMS", n"BlackwallMods", n"Console"];
  let tipos: array<gameGodModeType> = [gameGodModeType.Invulnerable, gameGodModeType.Immortal];
  for fonte in fontes {
    for tipo in tipos {
      let voltas: Int32 = 0;
      while voltas < 8 && sys.RemoveGodMode(id, tipo, fonte) {
        voltas += 1;
      };
    };
  };
  BwmsConfigSet("godmode", "0");
}

// Liga os dois tipos com a fonte `BWMS`, cada um so se ainda nao estiver (o save pode trazer a entrada e duplicar a fonte).
public func BwmsGodModeLigar(sys: ref<GodModeSystem>, id: EntityID) -> Void {
  if !sys.HasGodMode(id, gameGodModeType.Invulnerable) {
    sys.AddGodMode(id, gameGodModeType.Invulnerable, n"BWMS");
  };
  if !sys.HasGodMode(id, gameGodModeType.Immortal) {
    sys.AddGodMode(id, gameGodModeType.Immortal, n"BWMS");
  };
  BwmsConfigSet("godmode", "1");
}

// Entrada no jogo: o Rust chama isto uma vez por player de gameplay real (fim do load), com a GameInstance do próprio player. Disparador
// único do que o produto faz com o V vivo; pelo tick e não por wrapper de `OnGameAttached`, que também recebe o player do menu.
public func BwmsAoEntrarNoJogo(game: GameInstance) -> Void {
  let pp: ref<PlayerPuppet> = GameInstance.GetPlayerSystem(game).GetLocalPlayerControlledGameObject() as PlayerPuppet;
  if !IsDefined(pp) {
    return;
  };
  // O PuppetStateHandler do V (estado de braços e pés para os sufixos de aparência).
  pp.BwmsAlwaysRun_estado_do_boneco();
  // God Mode ligado na config externa (fora do save) volta. O save pode já trazer a entrada `BWMS`: só põe sem invulnerabilidade, para não
  // duplicar a fonte.
  let cfgGod: String = BwmsConfigGet("godmode");
  if Equals(cfgGod, "1") {
    BwmsGodModeLigar(GameInstance.GetGodModeSystem(game), pp.GetEntityID());
  };
  // Os cheats continuos do menu MODS voltam pela config, como o God Mode.
  if BwmsCheatLigado("cheat_npc") {
    pp.BWMSSetCloak(true);
  };
  if BwmsCheatLigado("cheat_municao") {
    pp.BWMSApplyMagazine();
  };
  if BwmsCheatLigado("cheat_pulo") {
    pp.BWMSSetMultiJump(true);
  };
  // A dilatacao do TimeSystem pode sobrar de um save carregado no meio do Sandevistan; o V novo nasce com ele desligado.
  pp.BWMSSandevistan(false);
  pp.BWMSCheatSchedule();
}

// i18n: mesma deteccao do bwms-settings-poc (idioma do jogo via /language OnScreen, default EN).
// Turco incluido porque o menu aparecia em ingles dentro de um jogo em turco.
@addMethod(PauseMenuGameController)
public func BWMSLang() -> Int32 {
  let v: ref<ConfigVarListName> =
    this.GetSystemRequestsHandler().GetUserSettings().GetVar(n"/language", n"OnScreen") as ConfigVarListName;
  if !IsDefined(v) { return 0; };
  let code: String = NameToString(v.GetValue());
  if StrBeginsWith(code, "pt") { return 1; };
  if StrBeginsWith(code, "zh") { return 2; };
  if StrBeginsWith(code, "tr") { return 3; };
  return 0;
}
@addMethod(PauseMenuGameController)
public func L(en: String, pt: String, zh: String, tr: String) -> String {
  switch this.BWMSLang() {
    case 1: return pt;
    case 2: return zh;
    case 3: return tr;
  };
  return en;
}

@wrapMethod(PauseMenuGameController)
private func PopulateMenuItemList() -> Void {
  wrappedMethod();
  this.AddMenuItem(this.L("MODS", "MODS", "模组", "MODLAR"), n"BWModsRoot");
  this.m_menuListController.Refresh();
}

@addMethod(PauseMenuGameController)
private final func BWHasGodMode() -> Bool {
  let owner: ref<GameObject> = this.GetPlayerControlledObject();
  if !IsDefined(owner) { return false; };
  let sys: ref<GodModeSystem> = GameInstance.GetGodModeSystem(owner.GetGame());
  return sys.HasGodMode(owner.GetEntityID(), gameGodModeType.Invulnerable)
      || sys.HasGodMode(owner.GetEntityID(), gameGodModeType.Immortal);
}

@addMethod(PauseMenuGameController)
private final func BWPlayer() -> ref<PlayerPuppet> {
  return this.GetPlayerControlledObject() as PlayerPuppet;
}

// Sufixo de estado para os itens que ligam/desligam. Mostrar o estado no proprio rotulo e o que
// o God Mode ja fazia; sem isso, apertar um toggle nao da retorno nenhum.
@addMethod(PauseMenuGameController)
private final func BWOnOff(on: Bool) -> String {
  return on ? this.L("ON", "LIGADO", "开", "AÇIK") : this.L("OFF", "DESLIGADO", "关", "KAPALI");
}

@addMethod(PauseMenuGameController)
private final func BWShowModsPage() -> Void {
  this.Clear();
  this.AddMenuItem(this.L("God Mode: ", "Modo Imortal: ", "上帝模式：", "Tanrı Modu: ") + this.BWOnOff(this.BWHasGodMode()), n"BWModsGod");
  this.AddMenuItem(this.L("Heal", "Curar", "治疗", "İyileştir"), n"BWModsHeal");
  this.AddMenuItem(this.L("+10,000 Eddies", "+10.000 Eddies", "+10,000 欧元币", "+10.000 Eddie"), n"BWModsMoney");
  this.AddMenuItem(this.L("Unlimited RAM: ", "RAM ilimitada: ", "无限内存：", "Sınırsız RAM: ")
    + this.BWOnOff(BwmsCheatLigado("cheat_ram")), n"BWModsRam");
  this.AddMenuItem(this.L("Unlimited ammo: ", "Municao ilimitada: ", "无限弹药：", "Sınırsız Mermi: ")
    + this.BWOnOff(BwmsCheatLigado("cheat_municao")), n"BWModsAmmo");
  this.AddMenuItem(this.L("NPCs leave you alone: ", "NPCs te deixam em paz: ", "NPC 不再攻击你：", "NPC'ler Saldırmasın: ")
    + this.BWOnOff(BwmsCheatLigado("cheat_npc")), n"BWModsCloak");
  this.AddMenuItem(this.L("Unlimited air jumps: ", "Pulos ilimitados no ar: ", "无限空中跳跃：", "Sınırsız Havada Zıplama: ")
    + this.BWOnOff(BwmsCheatLigado("cheat_pulo")), n"BWModsJump");
  this.AddMenuItem("Sandevistan (Option + Z): " + this.BWOnOff(BwmsCheatLigado("cheat_sande")), n"BWModsSande");
  this.AddMenuItem(this.L("Sandevistan world speed: ", "Velocidade do mundo no Sandevistan: ", "斯安威斯坦世界速度：", "Sandevistan dünya hızı: ")
    + "%" + BwmsSandevistanEscala(), n"BWModsSandeEscala");
  this.AddMenuItem(this.L("Map teleport (Option + X): ", "Teleporte no mapa (Option + X): ", "地图传送（Option + X）：", "Haritada ışınlanma (Option + X): ")
    + this.BWOnOff(BwmsCheatLigado("cheat_isinlan")), n"BWModsIsinlan");
  this.AddMenuItem(this.L("Back", "Voltar", "返回", "Geri"), n"BWModsBack");
  this.m_menuListController.Refresh();
  this.SetCursorOverWidget(inkCompoundRef.GetWidgetByIndex(this.m_menuList, 0), 0.00, true);
}

@wrapMethod(PauseMenuGameController)
protected cb func OnMenuItemActivated(index: Int32, target: ref<ListItemController>) -> Bool {
  let data: ref<PauseMenuListItemData> = target.GetData() as PauseMenuListItemData;
  if !IsDefined(data) { return wrappedMethod(index, target); };
  let owner: ref<GameObject> = this.GetPlayerControlledObject();
  let pp: ref<PlayerPuppet> = this.BWPlayer();

  if Equals(data.eventName, n"BWModsRoot") {
    this.PlaySound(n"Button", n"OnPress");
    this.m_bwInModsPage = true; this.BWShowModsPage(); return true;
  };
  if Equals(data.eventName, n"BWModsBack") {
    this.PlaySound(n"Button", n"OnPress");
    this.m_bwInModsPage = false; this.ShowActionsList(); return true;
  };
  if Equals(data.eventName, n"BWModsGod") {
    this.PlaySound(n"Button", n"OnPress");
    if IsDefined(owner) {
      let sys: ref<GodModeSystem> = GameInstance.GetGodModeSystem(owner.GetGame());
      if this.BWHasGodMode() {
        BwmsGodModeDesligar(sys, owner.GetEntityID());
      } else {
        BwmsGodModeLigar(sys, owner.GetEntityID());
        // O God Mode nao repoe o que ja foi perdido: sem isto, ligar com pouca vida deixa o
        // jogador ligado e quase morto, o que se parece com "nao funcionou".
        GameInstance.GetStatPoolsSystem(owner.GetGame())
          .RequestSettingStatPoolValue(Cast<StatsObjectID>(owner.GetEntityID()),
                                       gamedataStatPoolType.Health, 100.00, owner, true);
      };
    };
    this.BWShowModsPage(); return true;
  };
  if Equals(data.eventName, n"BWModsHeal") {
    this.PlaySound(n"Button", n"OnPress");
    if IsDefined(owner) {
      GameInstance.GetStatPoolsSystem(owner.GetGame())
        .RequestSettingStatPoolValue(Cast<StatsObjectID>(owner.GetEntityID()),
                                     gamedataStatPoolType.Health, 100.00, owner, true);
    };
    return true;
  };
  if Equals(data.eventName, n"BWModsMoney") {
    this.PlaySound(n"Button", n"OnPress");
    if IsDefined(owner) {
      GameInstance.GetTransactionSystem(owner.GetGame())
        .GiveItem(owner, ItemID.FromTDBID(t"Items.money"), 10000);
    };
    return true;
  };
  if Equals(data.eventName, n"BWModsRam") {
    this.PlaySound(n"Button", n"OnPress");
    if IsDefined(pp) {
      let on: Bool = !BwmsCheatLigado("cheat_ram");
      BwmsCheatGravar("cheat_ram", on);
      if on {
        // Enche na hora, sem esperar o primeiro tick: o retorno tem que ser imediato.
        pp.BWMSFillRam();
        pp.BWMSCheatSchedule();
      };
    };
    this.BWShowModsPage(); return true;
  };
  if Equals(data.eventName, n"BWModsAmmo") {
    this.PlaySound(n"Button", n"OnPress");
    if IsDefined(pp) {
      let on: Bool = !BwmsCheatLigado("cheat_municao");
      BwmsCheatGravar("cheat_municao", on);
      if on {
        pp.BWMSApplyMagazine();
        pp.BWMSCheatSchedule();
      } else {
        pp.BWMSClearMagazine();
      };
    };
    this.BWShowModsPage(); return true;
  };
  if Equals(data.eventName, n"BWModsCloak") {
    this.PlaySound(n"Button", n"OnPress");
    if IsDefined(pp) {
      pp.BWMSSetCloak(!BwmsCheatLigado("cheat_npc"));
    };
    this.BWShowModsPage(); return true;
  };
  if Equals(data.eventName, n"BWModsSande") {
    this.PlaySound(n"Button", n"OnPress");
    let on: Bool = !BwmsCheatLigado("cheat_sande");
    BwmsCheatGravar("cheat_sande", on);
    if !on && IsDefined(pp) && pp.m_bwSandeAtivo {
      pp.BWMSSandevistan(false);
    };
    this.BWShowModsPage(); return true;
  };
  if Equals(data.eventName, n"BWModsIsinlan") {
    this.PlaySound(n"Button", n"OnPress");
    BwmsCheatGravar("cheat_isinlan", !BwmsCheatLigado("cheat_isinlan"));
    this.BWShowModsPage(); return true;
  };
  if Equals(data.eventName, n"BWModsSandeEscala") {
    this.PlaySound(n"Button", n"OnPress");
    let e: String = BwmsSandevistanEscala();
    let prox: String = Equals(e, "0.001") ? "10" : (Equals(e, "10") ? "25" : (Equals(e, "25") ? "50" : (Equals(e, "50") ? "75" : "0.001")));
    BwmsConfigSet("sande_escala", prox);
    if IsDefined(pp) && pp.m_bwSandeAtivo {
      pp.BWMSSandevistan(true);
    };
    this.BWShowModsPage(); return true;
  };
  if Equals(data.eventName, n"BWModsJump") {
    this.PlaySound(n"Button", n"OnPress");
    if IsDefined(pp) {
      pp.BWMSSetMultiJump(!BwmsCheatLigado("cheat_pulo"));
    };
    this.BWShowModsPage(); return true;
  };

  return wrappedMethod(index, target);
}
