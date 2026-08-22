// codeware-districtresolver.reds — Codeware `DistrictResolver.GetDistrict`/`GetArea` (item #151
// do catálogo, `PENDENCIAS-UNIFICADAS.md`, 2026-08-11) — declaração PERMANENTE das natives, dado
// puro (tabela estática de 16 setores reais + algoritmo de "pular 3 bytes, comparar 7"), zero
// endereço nativo (ver `crate::district` no Rust, 6/6 testes offline).
native func BwmsDistrictGetDistrict(sectorName: CName) -> gamedataDistrict
native func BwmsDistrictGetArea(sectorName: CName) -> gamedataDistrict
