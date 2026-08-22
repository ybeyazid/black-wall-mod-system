//! capture.rs — captura de COR pro VR (Quest) DENTRO do dylib do console, em Rust puro.
//!
//! Roda no MESMO hook de present do overlay (`overlay::my_present` chama `on_present`),
//! então VR e CET-console rodam JUNTOS, com UM dylib injetado — sem `cpvr.js`, sem switcher.
//! É o porte do `presentCapture` do antigo `cpvr.js`: blita o `drawable.texture` (frame FINAL,
//! pós-composite da ink E pós-overlay do CET → COM HUD/menu/pausa) num MTLBuffer no cmdbuf do
//! present, lê o anterior (ping-pong) e grava `frame.color.raw` no stream_dir. fmt REAL no header
//! (o cpvr-stream ramifica: BGRA8/RGBA8 = sem tonemap; RG11B10 = HDR antigo).
//!
//! A DEPTH do mundo vem depois, num hook separado do `encodeToCommandBuffer:` do scaler MetalFX.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::sync::Mutex;
use std::time::Instant;

type Id = *mut c_void;
type Sel = *const c_void;

type Class = *mut c_void;
type Method = *mut c_void;
type Imp = *const c_void;

extern "C" {
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    fn objc_copyClassList(out_count: *mut u32) -> *mut Class;
    fn class_getName(cls: Class) -> *const c_char;
    fn class_getInstanceMethod(cls: Class, s: Sel) -> Method;
    fn method_getImplementation(m: Method) -> Imp;
    fn method_setImplementation(m: Method, imp: Imp) -> Imp;
    fn object_getClass(obj: Id) -> Class;
}

unsafe fn sel(name: &str) -> Sel {
    match CString::new(name) {
        Ok(c) => sel_registerName(c.as_ptr()),
        Err(_) => std::ptr::null(),
    }
}
unsafe fn msg0(r: Id, s: Sel) -> Id {
    let f: extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const c_void);
    f(r, s)
}
unsafe fn msg_usize(r: Id, s: Sel) -> usize {
    let f: extern "C" fn(Id, Sel) -> usize = std::mem::transmute(objc_msgSend as *const c_void);
    f(r, s)
}
unsafe fn new_buffer(dev: Id, len: usize) -> Id {
    let f: extern "C" fn(Id, Sel, usize, usize) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(dev, sel("newBufferWithLength:options:"), len, 0)
}
// MTLOrigin/MTLSize são structs de 24B → ABI arm64 passa por PONTEIRO (igual o cpvr.js fazia).
unsafe fn blit_copy(enc: Id, tex: Id, origin: *const u64, size: *const u64, buf: Id, bpr: usize) {
    let s = sel("copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:");
    let f: extern "C" fn(Id, Sel, Id, u64, u64, *const u64, *const u64, Id, u64, u64, u64) =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(enc, s, tex, 0, 0, origin, size, buf, 0, bpr as u64, (bpr * (size_h(size))) as u64);
}
#[inline]
unsafe fn size_h(size: *const u64) -> usize {
    *size.add(1) as usize
}
// variante COM options (depth/stencil precisa options=1 no blit).
unsafe fn blit_copy_opts(enc: Id, tex: Id, origin: *const u64, size: *const u64, buf: Id, bpr: usize) {
    let s = sel("copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:options:");
    let f: extern "C" fn(Id, Sel, Id, u64, u64, *const u64, *const u64, Id, u64, u64, u64, u64) =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(enc, s, tex, 0, 0, origin, size, buf, 0, bpr as u64, (bpr * size_h(size)) as u64, 1);
}

const RAM: &str = "/Volumes/CPVRRAM/";

/// Destino dos frames quando o RAM-disk não está montado.
///
/// Resolvido em RUNTIME a partir de `BWMS_GAME`, com fallback pro diretório temporário. Era uma
/// constante com o caminho da máquina de UMA pessoa (`/Volumes/<disco>/SteamLibrary/...`), o que
/// tinha dois defeitos: quebrava silenciosamente pra qualquer outra instalação (`std::fs::write`
/// falha e o erro é ignorado por design), e vazava o nome do volume do autor em todo binário
/// compilado. Um path morto já causou exatamente essa falha silenciosa em 2026-08-07.
fn log_dir() -> String {
    if let Ok(g) = std::env::var("BWMS_GAME") {
        if !g.is_empty() {
            return format!("{}/red4ext/logs/", g.trim_end_matches('/'));
        }
    }
    std::env::temp_dir().to_string_lossy().into_owned() + "/bwms-frames/"
}

struct Slot {
    buf: usize, // Id como usize (Send)
    cap: usize,
    w: usize,
    h: usize,
    fmt: usize,
    valid: bool,
}
impl Slot {
    const fn empty() -> Self {
        Slot { buf: 0, cap: 0, w: 0, h: 0, fmt: 0, valid: false }
    }
}
struct State {
    ring: [Slot; 2],
    last: Option<Instant>,
    seq: u32,
    enabled: Option<bool>,
}
static STATE: Mutex<State> = Mutex::new(State {
    ring: [Slot::empty(), Slot::empty()],
    last: None,
    seq: 0,
    enabled: None,
});

fn stream_dir() -> String {
    // RAM-disk quando montado (bem mais rápido pra stream de frame); senão o diretório de log
    // resolvido em runtime. Devolve `String` porque `log_dir()` não é `'static`.
    if std::path::Path::new(RAM).exists() { RAM.to_string() } else { log_dir() }
}

/// Chamado de `overlay::my_present` DEPOIS do render do imgui (pega game+UI+overlay).
pub unsafe fn on_present(cmd_buffer: Id, drawable: Id) {
    if drawable.is_null() {
        return;
    }
    let mut st = match STATE.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    // gate: só captura se o stream estiver armado (.cpvr-dump-arm). Lê uma vez.
    if st.enabled.is_none() {
        st.enabled = Some(std::path::Path::new(&format!("{}.cpvr-dump-arm", log_dir())).exists());
    }
    if st.enabled != Some(true) {
        return;
    }
    // throttle ~44ms (~22fps)
    let now = Instant::now();
    if let Some(t) = st.last {
        if now.duration_since(t).as_millis() < 44 {
            return;
        }
    }
    st.last = Some(now);

    let tex = msg0(drawable, sel("texture"));
    if tex.is_null() {
        return;
    }
    let w = msg_usize(tex, sel("width"));
    let h = msg_usize(tex, sel("height"));
    let fmt = msg_usize(tex, sel("pixelFormat"));
    if w == 0 || h == 0 {
        return;
    }
    let dev = msg0(tex, sel("device"));
    let bpr = w * 4;
    let len = bpr * h;
    let cur = (st.seq & 1) as usize;
    let prev = ((st.seq + 1) & 1) as usize;

    // (re)aloca o buffer do slot atual
    if st.ring[cur].buf == 0 || len > st.ring[cur].cap {
        let b = new_buffer(dev, len);
        st.ring[cur].buf = b as usize;
        st.ring[cur].cap = len;
    }
    if st.ring[cur].buf == 0 {
        return;
    }
    let enc = msg0(cmd_buffer, sel("blitCommandEncoder"));
    if enc.is_null() {
        return;
    }
    let origin: [u64; 3] = [0, 0, 0];
    let size: [u64; 3] = [w as u64, h as u64, 1];
    blit_copy(enc, tex, origin.as_ptr(), size.as_ptr(), st.ring[cur].buf as Id, bpr);
    msg0(enc, sel("endEncoding"));
    st.ring[cur].w = w;
    st.ring[cur].h = h;
    st.ring[cur].fmt = fmt;
    st.ring[cur].valid = true;

    // grava o frame ANTERIOR (a GPU já terminou)
    if st.seq >= 1 && st.ring[prev].buf != 0 && st.ring[prev].valid {
        let pbuf = st.ring[prev].buf as Id;
        let contents = msg0(pbuf, sel("contents"));
        if !contents.is_null() {
            let (w, h, fmt, plen) = {
                let p = &st.ring[prev];
                (p.w, p.h, p.fmt, p.w * 4 * p.h)
            };
            write_frame(&stream_dir(), st.seq - 1, w, h, fmt, contents as *const u8, plen);
        }
    }
    st.seq = st.seq.wrapping_add(1);
}

unsafe fn write_frame(dir: &str, seq: u32, w: usize, h: usize, fmt: usize, contents: *const u8, len: usize) {
    let body = std::slice::from_raw_parts(contents, len);
    let mut out = Vec::with_capacity(32 + len);
    out.extend_from_slice(&0x3156_5043u32.to_le_bytes()); // 'CPV1'
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(w as u32).to_le_bytes());
    out.extend_from_slice(&(h as u32).to_le_bytes());
    out.extend_from_slice(&(fmt as u32).to_le_bytes());
    out.extend_from_slice(&((w * 4) as u32).to_le_bytes());
    out.extend_from_slice(&(len as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(body);
    let path = format!("{dir}frame.color.raw");
    let tmp = format!("{path}.tmp");
    if std::fs::write(&tmp, &out).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

// ---------------------------------------------------------------- DEPTH (scaler MetalFX)
// Porta o caminho de depth do cpvr.js: swizzla `encodeToCommandBuffer:` nas classes cujo nome
// contém "MFXTemporalScaling"; no onLeave (após o orig), lê `depthTexture` do scaler e blita
// (Depth32Float_Stencil8, fmt 260, options=1) → frame.depth.raw. Sem isso o DIBR não tem
// profundidade do mundo (só serve pra menu 2D).

struct DState {
    ring: [Slot; 2],
    seq: u32,
    origs: Vec<(usize, usize)>, // (class as usize, orig IMP as usize)
    installed: bool,
}
static DSTATE: Mutex<DState> = Mutex::new(DState {
    ring: [Slot::empty(), Slot::empty()],
    seq: 0,
    origs: Vec::new(),
    installed: false,
});

/// Spawna uma thread que tenta achar+swizzlar a classe do scaler MetalFX (ela só existe
/// quando o jogo entra em gameplay com upscaling; por isso re-tenta).
pub fn install_scaler_hooks() {
    // GATE: depth off por padrão (o hook do scaler crashou no 1º teste — ver my_scaler_encode).
    // Liga só com o marcador `.cpvr-depth`. A captura de COR (present) NÃO depende disto.
    if !std::path::Path::new(&format!("{}.cpvr-depth", log_dir())).exists() {
        crate::log("[capture] depth: desligado (sem .cpvr-depth); só cor por ora");
        return;
    }
    std::thread::spawn(|| {
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_secs(3));
            unsafe {
                if try_install_scaler() {
                    crate::log("[capture] depth: hook do scaler MetalFX instalado");
                    return;
                }
            }
        }
        crate::log("[capture] depth: classe MFXTemporalScaling não achada (sem upscaling?)");
    });
}

unsafe fn try_install_scaler() -> bool {
    let mut st = match DSTATE.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    if st.installed {
        return true;
    }
    let mut count: u32 = 0;
    let list = objc_copyClassList(&mut count);
    if list.is_null() || count == 0 {
        return false;
    }
    let s_enc = sel("encodeToCommandBuffer:");
    let mut hooked = false;
    for i in 0..count as isize {
        let cls = *list.offset(i);
        if cls.is_null() {
            continue;
        }
        let np = class_getName(cls);
        if np.is_null() {
            continue;
        }
        let name = std::ffi::CStr::from_ptr(np).to_string_lossy();
        if !name.contains("MFXTemporalScaling") {
            continue;
        }
        let m = class_getInstanceMethod(cls, s_enc);
        if m.is_null() {
            continue;
        }
        let orig = method_getImplementation(m);
        st.origs.push((cls as usize, orig as usize));
        method_setImplementation(m, my_scaler_encode as Imp);
        hooked = true;
    }
    // objc_copyClassList aloca com malloc; vaza uma vez (ok, roda no máximo ~40x).
    if hooked {
        st.installed = true;
    }
    hooked
}

extern "C" fn my_scaler_encode(this: Id, cmd: Sel, cmdbuf: Id) {
    unsafe {
        // acha o orig pela classe do receiver e chama PRIMEIRO (o scaler renderiza)
        let cls = object_getClass(this) as usize;
        let orig = {
            match DSTATE.lock() {
                Ok(g) => g.origs.iter().find(|(c, _)| *c == cls).map(|(_, o)| *o),
                Err(_) => None,
            }
        };
        if let Some(o) = orig {
            let f: extern "C" fn(Id, Sel, Id) = std::mem::transmute(o as *const c_void);
            f(this, cmd, cmdbuf);
        }
        capture_depth(this, cmdbuf);
    }
}

unsafe fn capture_depth(scaler: Id, cmdbuf: Id) {
    if scaler.is_null() || cmdbuf.is_null() {
        return;
    }
    // gate igual à cor (.cpvr-dump-arm via STATE.enabled)
    if STATE.lock().map(|g| g.enabled).unwrap_or(None) != Some(true) {
        return;
    }
    let tex = msg0(scaler, sel("depthTexture"));
    if tex.is_null() {
        return; // getter intermitente; pula o frame (Quest reusa a última)
    }
    let w = msg_usize(tex, sel("width"));
    let h = msg_usize(tex, sel("height"));
    if w == 0 || h == 0 {
        return;
    }
    let dev = msg0(tex, sel("device"));
    let bpr = w * 4;
    let len = bpr * h;
    let mut st = match DSTATE.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let cur = (st.seq & 1) as usize;
    let prev = ((st.seq + 1) & 1) as usize;
    if st.ring[cur].buf == 0 || len > st.ring[cur].cap {
        let b = new_buffer(dev, len);
        st.ring[cur].buf = b as usize;
        st.ring[cur].cap = len;
    }
    if st.ring[cur].buf == 0 {
        return;
    }
    let enc = msg0(cmdbuf, sel("blitCommandEncoder"));
    if enc.is_null() {
        return;
    }
    let origin: [u64; 3] = [0, 0, 0];
    let size: [u64; 3] = [w as u64, h as u64, 1];
    blit_copy_opts(enc, tex, origin.as_ptr(), size.as_ptr(), st.ring[cur].buf as Id, bpr);
    msg0(enc, sel("endEncoding"));
    st.ring[cur].w = w;
    st.ring[cur].h = h;
    st.ring[cur].fmt = 260; // Depth32Float_Stencil8
    st.ring[cur].valid = true;
    if st.seq >= 1 && st.ring[prev].buf != 0 && st.ring[prev].valid {
        let pbuf = st.ring[prev].buf as Id;
        let contents = msg0(pbuf, sel("contents"));
        if !contents.is_null() {
            let (w, h, plen) = {
                let p = &st.ring[prev];
                (p.w, p.h, p.w * 4 * p.h)
            };
            write_depth(&stream_dir(), st.seq - 1, w, h, contents as *const u8, plen);
        }
    }
    st.seq = st.seq.wrapping_add(1);
}

unsafe fn write_depth(dir: &str, seq: u32, w: usize, h: usize, contents: *const u8, len: usize) {
    let body = std::slice::from_raw_parts(contents, len);
    let mut out = Vec::with_capacity(32 + len);
    out.extend_from_slice(&0x3156_5043u32.to_le_bytes());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(w as u32).to_le_bytes());
    out.extend_from_slice(&(h as u32).to_le_bytes());
    out.extend_from_slice(&260u32.to_le_bytes());
    out.extend_from_slice(&((w * 4) as u32).to_le_bytes());
    out.extend_from_slice(&(len as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(body);
    let path = format!("{dir}frame.depth.raw");
    let tmp = format!("{path}.tmp");
    if std::fs::write(&tmp, &out).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}
