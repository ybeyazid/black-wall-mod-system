//! bwms-helper — substitui `otool`/`python3` (que são Xcode Command Line Tools e
//! NÃO existem num Mac de fábrica) por um binário nativo arm64 auto-contido.
//! Faz só leitura/edição de Mach-O 64 thin (o binário do Cyberpunk no mac):
//!   has    <bin> <substr>  -> exit 0 se um LC_LOAD_DYLIB contém <substr>, senão 1
//!   info   <bin>           -> lista os caminhos de LC_LOAD_DYLIB (1 por linha)
//!   insert <bin> <dylib>   -> adiciona um LC_LOAD_DYLIB (na padding do header)
//! Sem dependências, sem CLT, roda no Mac liso. (codesign e xattr já são base.)

use std::fs;
use std::process::exit;

const MH_MAGIC_64: u32 = 0xFEED_FACF;
const LC_SEGMENT_64: u32 = 0x19;
const LC_LOAD_DYLIB: u32 = 0x0C;

fn rd_u32(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn str_at(d: &[u8], s: usize) -> (&[u8], usize) {
    let end = d[s..].iter().position(|&b| b == 0).map(|p| s + p).unwrap_or(s);
    (&d[s..end], end)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: bwms-helper <has|info|insert> <binary> [substr|dylib]");
        exit(2);
    }
    let cmd = args[1].as_str();
    let path = &args[2];
    let mut data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error reading {path}: {e}");
            exit(1);
        }
    };
    if data.len() < 32 || rd_u32(&data, 0) != MH_MAGIC_64 {
        eprintln!("not a thin 64-bit Mach-O (arm64)");
        exit(1);
    }
    let ncmds = rd_u32(&data, 16);
    let sizeofcmds = rd_u32(&data, 20) as usize;
    let lc_start = 32usize;
    let lc_end = lc_start + sizeofcmds;

    match cmd {
        "has" | "info" => {
            let needle = args.get(3).map(|s| s.as_str()).unwrap_or("");
            let mut off = lc_start;
            let mut found = false;
            for _ in 0..ncmds {
                if off + 8 > data.len() {
                    break;
                }
                let c = rd_u32(&data, off);
                let cs = rd_u32(&data, off + 4) as usize;
                if c == LC_LOAD_DYLIB {
                    let nameoff = rd_u32(&data, off + 8) as usize;
                    let (name, _) = str_at(&data, off + nameoff);
                    let name = String::from_utf8_lossy(name);
                    if cmd == "info" {
                        println!("{name}");
                    }
                    if !needle.is_empty() && name.contains(needle) {
                        found = true;
                    }
                }
                if cs == 0 {
                    break;
                }
                off += cs;
            }
            if cmd == "has" {
                exit(if found { 0 } else { 1 });
            }
        }
        "insert" => {
            let dylib = match args.get(3) {
                Some(s) => s.clone(),
                None => {
                    eprintln!("insert requires <dylib>");
                    exit(2);
                }
            };
            // 1a passada (só leitura): idempotência + headroom da padding.
            let mut headroom_limit = data.len();
            let mut off = lc_start;
            for _ in 0..ncmds {
                if off + 8 > data.len() {
                    break;
                }
                let c = rd_u32(&data, off);
                let cs = rd_u32(&data, off + 4) as usize;
                if c == LC_LOAD_DYLIB {
                    let nameoff = rd_u32(&data, off + 8) as usize;
                    let (name, _) = str_at(&data, off + nameoff);
                    if name == dylib.as_bytes() {
                        println!("already installed — nothing to do");
                        return;
                    }
                } else if c == LC_SEGMENT_64 {
                    let nsects = rd_u32(&data, off + 64) as usize;
                    for sx in 0..nsects {
                        let p = off + 72 + sx * 80 + 48;
                        if p + 4 <= data.len() {
                            let soff = rd_u32(&data, p) as usize;
                            if soff > 0 && soff < headroom_limit {
                                headroom_limit = soff;
                            }
                        }
                    }
                }
                if cs == 0 {
                    break;
                }
                off += cs;
            }
            let mut name = dylib.into_bytes();
            name.push(0);
            let cmdsize_new = (24 + name.len() + 7) & !7;
            if headroom_limit < lc_end || headroom_limit - lc_end < cmdsize_new {
                eprintln!("no room in the header (needs {cmdsize_new}B). Restore via Steam.");
                exit(1);
            }
            let mut lc: Vec<u8> = Vec::with_capacity(cmdsize_new);
            lc.extend_from_slice(&LC_LOAD_DYLIB.to_le_bytes());
            lc.extend_from_slice(&(cmdsize_new as u32).to_le_bytes());
            lc.extend_from_slice(&24u32.to_le_bytes()); // name.offset
            lc.extend_from_slice(&2u32.to_le_bytes()); // timestamp
            lc.extend_from_slice(&0x0001_0000u32.to_le_bytes()); // current_version
            lc.extend_from_slice(&0x0001_0000u32.to_le_bytes()); // compat_version
            lc.extend_from_slice(&name);
            lc.resize(cmdsize_new, 0);
            // OVERWRITE a padding (sem deslocar o arquivo — fileoffs continuam validos)
            data[lc_end..lc_end + cmdsize_new].copy_from_slice(&lc);
            data[16..20].copy_from_slice(&(ncmds + 1).to_le_bytes());
            data[20..24].copy_from_slice(&((sizeofcmds + cmdsize_new) as u32).to_le_bytes());
            // Escreve via temp + rename (replace atômico do inode): truncar IN-PLACE um
            // Mach-O já assinado e VALIDADO pelo kernel (binário já lançado uma vez) dá
            // EPERM por proteção de code-signing. Trocar o inode contorna — o dir só
            // precisa ser writable (já é). Vale Steam e GOG.
            let tmp = format!("{path}.bwms-tmp");
            if let Err(e) = fs::write(&tmp, &data) {
                eprintln!("error writing temp: {e}");
                exit(1);
            }
            // CRÍTICO: fs::write cria o arquivo 0644 (SEM bit de execução). Renomear por cima
            // deixaria o binário não-executável → launchd "spawn failed" (erro 111) / zsh
            // "permission denied". Copia o modo do ORIGINAL (que é executável) pro temp.
            match fs::metadata(path).map(|m| m.permissions()) {
                Ok(perm) => {
                    if let Err(e) = fs::set_permissions(&tmp, perm) {
                        let _ = fs::remove_file(&tmp);
                        eprintln!("error setting permissions: {e}");
                        exit(1);
                    }
                }
                Err(e) => {
                    let _ = fs::remove_file(&tmp);
                    eprintln!("error reading original permissions: {e}");
                    exit(1);
                }
            }
            if let Err(e) = fs::rename(&tmp, path) {
                let _ = fs::remove_file(&tmp);
                eprintln!("error replacing binary: {e}");
                exit(1);
            }
            println!("OK: load command added (cmdsize={cmdsize_new})");
        }
        other => {
            eprintln!("unknown command: {other}");
            exit(2);
        }
    }
}
