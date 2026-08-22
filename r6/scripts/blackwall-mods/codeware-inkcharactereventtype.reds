// inkCharacterEventType — enum da RTTI do PRÓPRIO JOGO.
//
// Não é criação de framework de mod nenhum: são os valores que a engine do Cyberpunk 2077 usa
// no evento de digitação. Qualquer implementação independente que queira ler esse evento tem
// que declarar exatamente estes nomes e estes números — é a interface do motor, não código
// autoral de terceiro. Declarado aqui porque o compilador redscript precisa do tipo para o
// `GetType()` de `inkCharacterEvent` (a implementação por trás é 100% Rust, própria).
public enum inkCharacterEventType {
  CharInput = 0,
  MoveCaretForward = 1,
  MoveCaretBackward = 2,
  Delete = 3,
  Backspace = 4,
}
