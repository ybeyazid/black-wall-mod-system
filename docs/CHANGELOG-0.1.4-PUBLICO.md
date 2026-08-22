# bwms 0.1.4 BETA — o que mudou

Versão anterior: 0.1.3 BETA.

## Resumo

A 0.1.3 shipava **15** arquivos de script; a 0.1.4 shipa **75**. São 60 arquivos novos — quase
todo o trabalho de compatibilidade acumulado desde a versão anterior finalmente chega ao pacote
público, em vez de ficar só na árvore de desenvolvimento.

## Correções de UI para quem escreve mod (última rodada antes de publicar)

Três defeitos reais na camada de UI equivalente à do Codeware, achados testando o jogo de verdade.
Os três eram silenciosos — zero crash, zero mensagem de erro, só a coisa não funcionando:

- **Botão custom não respondia a hover/clique.** Os callbacks de widget
  (`RegisterToCallback`/`UnregisterFromCallback`/`CallCustomCallback` de `inkCustomController`)
  estavam sendo registrados no game controller em vez do widget-raiz. Eventos de widget
  (`OnEnter`, `OnLeave`, `OnPress`, `OnRelease`, `OnBtnClick`) simplesmente nunca disparavam.
- **Callback registrado cedo demais era perdido.** Quem chamava `RegisterToCallback` antes do
  widget-raiz existir (num `Create()`, por exemplo) tinha o registro descartado em silêncio. Agora
  fica numa fila e é aplicado assim que o widget aparece.
- **`PopupButton` perdia o próprio controle de input.** `SetInputAction(...)` escrevia num objeto
  que já tinha sido liberado, e `GetInputAction()` voltava vazio — o que, na prática, quebrava o
  reconhecimento automático de Confirmar/Cancelar num popup de menu.

Com isso, um popup de menu com botões agora liga Confirmar/Cancelar sozinho pela ação de input de
cada botão, e o clique chega ao seu código — testado em jogo, com o resultado do popup conferido.

## Motor: uma trava de segurança a mais para plugins

`tweakdb_get_flat` / `tweakdb_set_flat` (API de plugin nativo) podiam derrubar o processo quando
chamadas fora do jogo — por exemplo, um plugin carregado por uma ferramenta, ou uma bateria de
testes do próprio autor do plugin. A camada de TweakDB agora confirma que está mesmo dentro do
Cyberpunk 2077 antes de tocar em qualquer endereço do motor, e devolve "não deu" em vez de morrer.
Achado pela suíte de testes do projeto, que passou a rodar 300/300.

## Limpeza do pacote

Cinco arquivos que eram só diagnóstico interno saíram do pacote público: estavam declarando
funções que nada chamava (código morto para quem instala) e escrevendo linhas de log de teste.
Nenhuma capacidade foi removida por essa limpeza — o que saiu não fazia nada no seu jogo.

## Originalidade: 31 arquivos removidos, e um portão para que não volte

Uma auditoria antes de publicar encontrou 31 arquivos redscript que eram porte literal de um
framework de terceiro (licença MIT). Em vez de publicá-los com atribuição, **foram removidos** —
a regra do projeto é que código de terceiro não viaja nele, com licença permissiva ou sem. Isso
custou capacidade: a camada de UI equivalente (popups, botões, campo de texto, controllers) não
vai nesta versão. O que for reescrito no futuro será escrito a partir do comportamento, não da
fonte alheia.

Para que isso não volte por descuido, `dist/auditoria-originalidade.py` agora roda como portão
obrigatório no empacotamento e no export público: ele compara cada arquivo publicado com as
fontes de terceiro e **falha a build** se achar um bloco contíguo de corpo copiado. Assinatura
sem corpo passa de propósito — é a forma do encaixe que um mod chama, e qualquer implementação
compatível declara igual. Detalhes em `docs/ORIGINALITY.md`.

## Seus saves não viajam mais no binário

O dylib embutia ~380 KB de hashes extraídos dos saves de uma pessoa — dado do jogo, de uma
partida específica, dentro do pacote de todo mundo. Saiu. A varredura que usava esses hashes
agora lê a lista do save de **quem está jogando**, em runtime; sem o arquivo, ela explica o que
falta em vez de falhar calada. O binário público encolheu 394 KB no processo.

Por framework equivalente:

| área | arquivos novos | o que abre pra quem escreve mod |
|---|---:|---|
| Codeware | 74 | reflexão, CallbackSystem, eventos/targets, UI (popups, controllers, widgets), localização, sistemas de entidade/mundo, utilitários (bits/hash/string/número/casts) |
| ArchiveXL | 7 | facade, estado de puppet, customização, mappins, journal/tracking |
| TweakXL | 4 | `TweakDBManager`/`TweakDBBatch` (CRUD de TweakDB) + tweaks scriptáveis |
| RED4ext | 1 | ponte de plugin |
| infra bwms | 7 | configurações de mod, transmog, utilitários internos |

## Motor (dylib)

- Build público confirmado **sem** injeção de evento de teclado (`CGEventPost`/
  `CGEventCreateKeyboardEvent` = 0 ocorrências) e **sem** escrita de log em disco. Essas são
  ferramentas de desenvolvimento; não têm por que existir num binário de usuário, e a ausência
  delas também reduz falso-positivo de antivírus.
- Gate de privacidade do empacotador reforçado: além de varrer strings, agora confere também os
  campos `name` dos load commands (`LC_ID_DYLIB`/`LC_LOAD_DYLIB`), onde um path de build já vazou
  no passado sem o scan de strings enxergar.

## Compatibilidade e segurança

- O conjunto de scripts foi validado compilando **isolado**, exatamente com os 75 arquivos que
  vão no pacote e nada mais — sem nenhum arquivo de teste. Isso importa: uma versão anterior
  travava o jogo justamente por levar dezenas de testes que se encadeavam no mesmo ponto de
  inicialização. O pacote também não leva nenhum gancho no `OnGameAttached`, o ponto onde esse
  encadeamento acontecia.
- Instalador procura primeiro a pasta da 0.1.4, com as anteriores como fallback.
- Permissões de execução conferidas em todos os `.command`/`.sh` (um deslize nisso já quebrou o
  duplo-clique de instalação numa versão passada).

## Honestidade sobre o estado

Isto é **beta**. Uma parte das capacidades acima está provada em jogo real; outra parte está
implementada e compila, mas ainda sem prova ao vivo registrada. O projeto mantém essa distinção
explícita nos próprios documentos internos e não conta as duas como a mesma coisa.

Se algo quebrar, o `extras/bwms-report.command` gera um relatório pronto pra colar num report.
