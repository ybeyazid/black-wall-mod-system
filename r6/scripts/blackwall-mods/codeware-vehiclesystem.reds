// codeware-vehiclesystem.reds — Codeware `Vehicle/VehicleSystem.reds` (catálogo item #72,
// round 3 do catálogo exaustivo Codeware, 2026-08-12, agente offline dedicado).
//
// `ToggleGarageVehicle` já tinha endereço RE'd e CONFIRMADO ao vivo desde 2026-07-29 — mas só
// como alvo de HOOK OBSERVE-ONLY (`selftest.rs::VEHICLESYSTEM_TOGGLEGARAGEVEHICLE_VM`), nunca
// exposto como native genuinamente chamável do redscript. Nenhum mod real conseguia disparar
// isso de propósito até agora. `BwmsVehicleSystemToggleGarageVehicle` chama o MESMO endereço
// já confirmado, com a MESMA ABI já documentada (`cp77-console/src/register.rs::
// tramp_vehiclesystem_toggle_garage_vehicle`) — zero RE nova.
//
// Divergência de assinatura DELIBERADA vs a fonte real: o Codeware passa um `GarageVehicleID`
// (struct de 2 campos: `recordID: TweakDBID`+`name: CName`) por referência; nossa native pega
// `recordID: TweakDBID` direto e monta o `name` (sempre zerado) do lado Rust — evita a
// complexidade de marshalling de struct-por-referência sem ganho prático (a fonte real também
// só popula `recordID` em todo uso conhecido, `name` fica default).
//
// ⚠️ RISCO RECONHECIDO (não é leitura pura de campo, ao contrário de `codeware-vehicleobject.
// reds`): CHAMA código real do motor que ativa/desativa um veículo do garage do player —
// mutação genuína, não observação. Por isso `EnablePlayerVehicleID` abaixo (cópia quase
// verbatim da fonte real, só trocando `ToggleGarageVehicle(garageID,...)` pela nossa native
// mais simples) NÃO tem smoke test automático que dispare sozinho — fica pronta pro próximo
// boot testar deliberadamente contra um veículo real do player, nunca via `@wrapMethod
// OnGameAttached` automático (mesma disciplina já usada pra qualquer native que muta estado
// real do mundo neste projeto, ex. `chunkmaskover`/`RefreshAppearance`).
//
// ⚠️ REGRA DE OURO: nada abaixo foi testado ao vivo. Item `#72` continua REAL_GAP até um boot
// confirmar `ToggleGarageVehicle` disparando de verdade via esta via redscript-callable (o
// endereço em si já foi visto disparando via OBSERVAÇÃO em 2026-07-29 — o que falta é provar
// que CHAMÁ-LO ativamente, com a ABI que reconstruímos, produz o mesmo efeito real).

native func BwmsVehicleSystemToggleGarageVehicle(sys: ref<VehicleSystem>, recordID: TweakDBID, enable: Bool) -> Bool

@addMethod(VehicleSystem)
public func BwmsToggleGarageVehicle(recordID: TweakDBID, enable: Bool) -> Bool {
    return BwmsVehicleSystemToggleGarageVehicle(this, recordID, enable);
}

// Cópia quase verbatim de `enablers/Codeware/scripts/Vehicle/VehicleSystem.reds::
// EnablePlayerVehicleID` — mesma lógica (checa posse via `Vehicle.vehicle_list.list`, ativa/
// desativa, despawna se pedido), só trocando a chamada nativa pela nossa (`BwmsToggleGarageVehicle`
// em vez do `ToggleGarageVehicle(GarageVehicleID,...)` literal do Codeware).
@addMethod(VehicleSystem)
public func EnablePlayerVehicleID(vehicleID: TweakDBID, enable: Bool, opt despawnIfDisabling: Bool) -> Bool {
    let playerVehicles: array<TweakDBID> = TweakDBInterface.GetForeignKeyArray(t"Vehicle.vehicle_list.list");

    if !ArrayContains(playerVehicles, vehicleID) {
        return false;
    }

    let success: Bool = this.BwmsToggleGarageVehicle(vehicleID, enable);

    if success && !enable && despawnIfDisabling {
        let garageID: GarageVehicleID;
        garageID.recordID = vehicleID;
        this.DespawnPlayerVehicle(garageID);
    }

    return success;
}
