//! bwms-catalog — candidatos de RE prontos pra investigar (ranking do catálogo exaustivo) +
//! checagem de native-no-dylib + bookkeeping de touched-today.tsv. Fonte única reusada pelo
//! `bwms-ai-agent` (tools `proximo_candidato`/`native_no_dylib`) e pelo binário `native-check`
//! (uso manual, mesma lógica que o `check-native-deployed.sh` aposentado usava via bash).

mod candidates;
mod game;
mod native;
mod touched;

pub use candidates::{all_real_gap_items, next_candidates, Candidate, SUPPORTED_FRAMEWORKS};
pub use game::{resolve_game_root, resolve_game_root_from};
pub use native::{contains_token, native_status, native_status_opt};
pub use touched::{append_touched, load_touched};
