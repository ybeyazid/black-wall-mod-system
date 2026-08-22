// BWMS — Codeware `#23` (`JobQueue`, `PENDENCIAS-UNIFICADAS.md`), 2026-08-14.
//
// Fonte real (`enablers/Codeware/src/App/Engine/JobQueue.hpp`): `App::JobQueue : Red::
// IScriptable` — `Create(opt group)` fábrica devolvendo `ref<JobQueue>`, `DispatchJob(job, opt
// size, opt batch)` empacota o job num `Red::JobQueue` (thread pool REAL do motor), `DispatchWait`/
// `Finalize`/`SyncWaitUntilFinished` sincronizam contra um `Red::JobHandle`/`Red::JobGroup` opacos.
// Infraestrutura de thread pool C++ que este projeto nunca teve endereço nativo pra (mesma
// ausência já documentada pro `ResourceToken::GetJobHandle`, `codeware-resourcetoken.reds`).
//
// DIVERGÊNCIA DE ESCOPO CONSCIENTE (ver nota completa em `register.rs::register_jobqueue_facade`):
// exposta como Facade 100%-ESTÁTICA (mesmo padrão de `TweakXL`/`TweakDBManager`, zero instância,
// zero `Create()`/`ref<JobQueue>`) — `DispatchJob(job)` chama o método `Handle()` do job
// SINCRONAMENTE (composição já provada: `rtti::class_of`+`resolve_in_class`+`call_func`, o mesmo
// núcleo de `BwmsCallMethod`/CallbackSystem), sem thread pool real e sem a forma batch
// (`aJobSize`/`aBatchSize`, `Handle(from,to)`+`Finish`). `SyncWaitUntilFinished` é NO-OP (o
// trabalho já terminou antes de `DispatchJob` devolver). `JobHandle`/`JobGroup`/`DispatchWait`/
// `Finalize` ficam de fora — mesma decisão já tomada pro `ResourceToken`.
//
// Capacidade prática entregue: um mod empacota um objeto com método `Handle()` (zero argumento) e
// chama `JobQueue.DispatchJob(job)` pra rodá-lo sem precisar construir/gerenciar um `JobQueue`
// próprio. `true` volta se o objeto foi resolvido e `Handle()` foi chamado; `false` se o objeto é
// nulo/inválido ou não tem método `Handle()`.
//
// CODADO, prova ao vivo pendente (regra de ouro: nunca fechar sem boot real).
public abstract native class JobQueue {
  public static native func DispatchJob(job: ref<IScriptable>) -> Bool;
  public static native func SyncWaitUntilFinished(opt timeout: Uint32) -> Void;
}
