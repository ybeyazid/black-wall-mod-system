// codeware-vehicleobject.reds — Codeware `Vehicle/VehicleObject.reds` (catálogo item #71,
// round 3 do catálogo exaustivo Codeware, 2026-08-12, agente offline dedicado).
//
// DIVERGÊNCIA DE ESCOPO CONSCIENTE (documentada, mesmo padrão já usado dezenas de vezes neste
// projeto — ver `#20`/`#29`/`#30` no catálogo): a fonte real do Codeware
// (`scripts/Vehicle/VehicleObject.reds`) expõe estes 5 campos via
// `@addField(VehicleObject) public native let X` — reivindicando uma property RTTI que só
// existe de verdade no Windows porque o C++ do Codeware a REGISTRA em runtime
// (`src/App/Entity/VehicleObjectEx.hpp`: `RTTI_EXPAND_CLASS(Red::vehicleBaseObject, {
// RTTI_PROPERTY(isOnGround); RTTI_PROPERTY(acceleration); RTTI_PROPERTY(deceleration);
// RTTI_PROPERTY(isReversing); RTTI_PROPERTY(burnout); });`). No Mac essa property NUNCA é
// registrada (não rodamos o C++ do Codeware) — declarar como `native let` arriscaria o mesmo
// crash de bind já documentado pro `#30`/CMesh (validação de ancestralidade RTTI em runtime).
//
// Em vez disso: GETTERS via função nativa lendo o offset real diretamente da memória do
// objeto — mesmo padrão seguro já estabelecido no projeto. Offsets Windows CONFIRMADOS pelo
// header vendorizado OFICIAL (`RED4ext.SDK/include/RED4ext/Scripting/Natives/
// vehicleBaseObject.hpp`, `RED4EXT_ASSERT_OFFSET`, não chutados) — campo de DADO, porta 1:1
// Windows->Mac sem shift Itanium (regra já estabelecida, reconfirmada hoje pro `#81`/
// WeatherSystem e `#168`/ComponentWrapper):
//   isOnGround   : bool  @ +0x25C
//   acceleration : float @ +0x264
//   deceleration : float @ +0x268
//   isReversing  : bool  @ +0x2A3
//   burnout      : float @ +0x2BC
//
// `VehicleObject` confirmado vanilla real e redscript-visível: é o tipo de retorno da native
// vanilla `GetMountedVehicle(object: ref<GameObject>) -> wref<VehicleObject>`
// (`core/gameplay/vehicles.script:1490`, já usada por scripts reais do jogo:
// `pocketRadio.script`/`autoDriveSystem.script`).
//
// ⚠️ REGRA DE OURO: nada abaixo foi testado ao vivo (offsets confirmados por header oficial,
// nunca contra memória viva desta build Mac). Item `#71` continua REAL_GAP até um boot
// confirmar contra um `VehicleObject*` real (player dentro de um veículo).

native func BwmsVehicleObjectGetIsOnGround(veh: ref<VehicleObject>) -> Bool
native func BwmsVehicleObjectGetAcceleration(veh: ref<VehicleObject>) -> Float
native func BwmsVehicleObjectGetDeceleration(veh: ref<VehicleObject>) -> Float
native func BwmsVehicleObjectGetIsReversing(veh: ref<VehicleObject>) -> Bool
native func BwmsVehicleObjectGetBurnout(veh: ref<VehicleObject>) -> Float

@addMethod(VehicleObject)
public func GetIsOnGround() -> Bool {
    return BwmsVehicleObjectGetIsOnGround(this);
}

@addMethod(VehicleObject)
public func GetAcceleration() -> Float {
    return BwmsVehicleObjectGetAcceleration(this);
}

@addMethod(VehicleObject)
public func GetDeceleration() -> Float {
    return BwmsVehicleObjectGetDeceleration(this);
}

@addMethod(VehicleObject)
public func GetIsReversing() -> Bool {
    return BwmsVehicleObjectGetIsReversing(this);
}

@addMethod(VehicleObject)
public func GetBurnout() -> Float {
    return BwmsVehicleObjectGetBurnout(this);
}
