//! bwms-proceed — helper EXTERNO que pula a tela "APERTE [espaço] PARA CONTINUAR" (attract) do boot
//! do Cyberpunk 2077 no macOS, injetando SPACE no processo do jogo.
//!
//! POR QUE EXTERNO: o proceed do attract é input HID nativo. Injeção de DENTRO do jogo é ignorada
//! pelo macOS (proteção anti-loop: um processo não injeta CGEvent pra si mesmo). Um processo
//! separado (este) COM Acessibilidade injeta e o jogo aceita — igual ao teclado real.
//!
//! COMO SABE A HORA: o BWMS (dylib) marca no /tmp/cp77-console.log a linha "engagement ativa = true"
//! (via redscript, quando a EngagementScreenGameController inicializa) e "= false" quando sai pro
//! menu. Este helper lê o log: injeta SPACE enquanto ativa; para no "= false", no fim do jogo, ou cap.
//! Fallback: se o log não marcar até ENGAGEMENT_TIMEOUT, injeta por tempo (heurística).
//!
//! SETUP (1x): dar Acessibilidade a ESTE binário em Ajustes > Privacidade e Segurança >
//! Acessibilidade. USO: rodar junto do jogo (ex.: `nohup bwms-proceed &` no launch, ou pelo
//! MODO-BWMS/fastboot). Sai sozinho quando o jogo fecha.

use std::ffi::c_void;
use std::io::{Read, Seek, SeekFrom};
use std::process::Command;
use std::time::{Duration, Instant};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceCreate(state: i32) -> *mut c_void;
    fn CGEventCreateKeyboardEvent(source: *mut c_void, keycode: u16, keydown: bool) -> *mut c_void;
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGEventPostToPid(pid: i32, event: *mut c_void);
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut c_void);
}

const LOG: &str = "/tmp/cp77-console.log";
// A tela mostra "APERTE [▬] PARA CONTINUAR" — o ícone é a BARRA DE ESPAÇO (não a letra "E"). O
// proceed avança com SPACE (keyCode 49), PROVADO in-game 2026-07-02. (O "E"/keyCode 14 NÃO avança.)
const KEY_SPACE: u16 = 49;
const POLL: Duration = Duration::from_millis(400);
const INJECT_EVERY: Duration = Duration::from_millis(1500);
const ENGAGEMENT_TIMEOUT: Duration = Duration::from_secs(70); // fallback se o log não marcar
const MAX_INJECTS: u32 = 12;

/// PID do processo do jogo (None se fechado).
fn game_pid() -> Option<i32> {
    let out = Command::new("pgrep").arg("-f").arg("Cyberpunk2077.app").output().ok()?;
    String::from_utf8_lossy(&out.stdout).split_whitespace().next()?.parse().ok()
}

/// Injeta keyDown+keyUp de `keycode`. PRINCIPAL: CGEventPost(kCGHIDEventTap) — injeta no HID stream
/// (o nível que o CP2077 lê, igual ao teclado real); vai pro app FRONTMOST, então o jogo precisa
/// estar em foco. FALLBACK: CGEventPostToPid pro `pid` do jogo. Externo + Acessibilidade → aceito.
fn press(pid: i32, keycode: u16) {
    unsafe {
        let src = CGEventSourceCreate(1); // kCGEventSourceStateHIDSystemState
        let down = CGEventCreateKeyboardEvent(src, keycode, true);
        let up = CGEventCreateKeyboardEvent(src, keycode, false);
        if !down.is_null() {
            CGEventPost(0, down); // 0 = kCGHIDEventTap (frontmost app)
            CGEventPostToPid(pid, down);
            CFRelease(down);
        }
        if !up.is_null() {
            CGEventPost(0, up);
            CGEventPostToPid(pid, up);
            CFRelease(up);
        }
        if !src.is_null() {
            CFRelease(src);
        }
    }
}

/// Lê o que foi anexado ao log desde `pos`; devolve o texto novo e atualiza `pos`.
fn read_new(pos: &mut u64) -> String {
    let Ok(mut f) = std::fs::File::open(LOG) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len < *pos {
        *pos = 0; // log rotacionou/reiniciou
    }
    if f.seek(SeekFrom::Start(*pos)).is_err() {
        return String::new();
    }
    let mut s = String::new();
    let _ = f.read_to_string(&mut s);
    *pos = len;
    s
}

fn main() {
    // modo "blast N": injeta SPACE N vezes espaçadas no jogo vivo (testar/passar telas HID travadas).
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(|s| s == "blast").unwrap_or(false) {
        let n: u32 = argv.get(2).and_then(|s| s.parse().ok()).unwrap_or(12);
        let pid = match game_pid() { Some(p) => p, None => { eprintln!("[blast] jogo nao achado"); return; } };
        eprintln!("[blast] injetando SPACE {n}x no pid {pid}");
        for i in 0..n {
            press(pid, KEY_SPACE);
            eprintln!("[blast] SPACE #{}", i + 1);
            std::thread::sleep(INJECT_EVERY);
        }
        return;
    }
    eprintln!("[bwms-proceed] esperando o Cyberpunk...");
    // 1) espera o jogo subir
    let mut pid;
    loop {
        if let Some(p) = game_pid() {
            pid = p;
            break;
        }
        std::thread::sleep(POLL);
    }
    eprintln!("[bwms-proceed] jogo pid={pid}. Vigiando a engagement no log.");

    // começa a ler o log DO FIM (só eventos deste boot)
    let mut pos = std::fs::metadata(LOG).map(|m| m.len()).unwrap_or(0);
    let start = Instant::now();
    let mut active = false;
    let mut injects = 0u32;
    let mut last_inject = Instant::now() - INJECT_EVERY;

    loop {
        // jogo fechou → fim
        match game_pid() {
            Some(p) => pid = p,
            None => {
                eprintln!("[bwms-proceed] jogo fechou. Saindo.");
                return;
            }
        }
        // consome novidades do log
        let chunk = read_new(&mut pos);
        if chunk.contains("engagement ativa = true") {
            if !active {
                eprintln!("[bwms-proceed] engagement ATIVA — injetando SPACE.");
            }
            active = true;
        }
        if chunk.contains("engagement ativa = false") {
            eprintln!("[bwms-proceed] engagement encerrou (menu). Injeções={injects}. Saindo.");
            return;
        }
        // fallback: sem marca do redscript por muito tempo → assume engagement e injeta por tempo
        if !active && start.elapsed() > ENGAGEMENT_TIMEOUT {
            eprintln!("[bwms-proceed] timeout sem marca do redscript — fallback por tempo.");
            active = true;
        }
        // injeta espaçado, até o cap
        if active && last_inject.elapsed() >= INJECT_EVERY {
            if injects >= MAX_INJECTS {
                eprintln!("[bwms-proceed] cap de {MAX_INJECTS} injeções atingido. Saindo.");
                return;
            }
            press(pid, KEY_SPACE);
            injects += 1;
            last_inject = Instant::now();
            eprintln!("[bwms-proceed] injetei SPACE #{injects} -> pid {pid}");
        }
        std::thread::sleep(POLL);
    }
}
