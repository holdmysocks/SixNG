//! [`ArchiveCipher`] backed by the game executable of the user's install.
//!
//! The executable is mapped into memory as an image (it is never started) and
//! a few of its routines are called. Only the C memory routines those use are
//! provided; every other import leads to a trap.
//!
//! Windows x64 with AVX2 only, and only for the game build whose routine
//! addresses are listed below.
use crate::rpf9toc::ArchiveCipher;
use anyhow::Result;
use std::path::Path;

/// The cipher of the install at `game`.
pub fn open(game: &Path) -> Result<Box<dyn ArchiveCipher>> {
    imp::open(game)
}

/// Whether this build of SixNG can read tables of contents at all.
pub const SUPPORTED: bool = cfg!(all(windows, target_arch = "x86_64"));

#[cfg(not(all(windows, target_arch = "x86_64")))]
mod imp {
    use super::*;
    pub fn open(_game: &Path) -> Result<Box<dyn ArchiveCipher>> {
        anyhow::bail!("reading the table of contents needs the game executable, which only works on Windows x64")
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod imp {
    use super::*;
    use anyhow::{anyhow, bail, ensure, Context};
    use std::collections::HashMap;
    use std::ffi::c_void;

    /// The build the addresses below belong to (PE header time stamp and image size).
    const BUILD_STAMP: u32 = 0x6af7_83d4;
    const BUILD_IMAGE_SIZE: u32 = 0x0b06_d000;
    /// Every routine returns this on success.
    const OK: u32 = 0x24a1_87b9;
    /// Key list: KEY_COUNT x { u64 (id << 32 | flag << 24), u64 address, u64 length }.
    const KEY_LIST: usize = 0x66e_5900;
    const KEY_COUNT: usize = 0x18;
    const KEY_FLAG: u8 = 2;
    /// Block cipher (table of contents).
    const BLOCK_SETKEY: usize = 0x55d_c660;
    const BLOCK_SETIV: usize = 0x55d_ba30;
    const BLOCK_DECRYPT: usize = 0x55d_bcf0;
    /// Stream cipher (file data).
    const STREAM_SETKEY: usize = 0x55d_c890;
    const STREAM_SETIV: usize = 0x55d_bf40;
    const STREAM_DECRYPT: usize = 0x55d_c010;
    /// The game's allocator entry points, redirected to the process heap.
    const GAME_ALLOC: usize = 0x9_0830;
    const GAME_FREE: usize = 0xf_c310;
    /// Built-in header keys by selector: (key id, address of the IV).
    const HEADER_KEYS: [(u8, u32, usize); 2] = [(1, 1, 0x62f_d460), (4, 0x1000_0001, 0x62f_d470)];

    const MEM_COMMIT_RESERVE: u32 = 0x3000;
    const MEM_RELEASE: u32 = 0x8000;
    const PAGE_EXECUTE_READWRITE: u32 = 0x40;
    const HEAP_ZERO_MEMORY: u32 = 8;

    #[link(name = "kernel32")]
    extern "system" {
        fn VirtualAlloc(addr: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
        fn VirtualFree(addr: *mut c_void, size: usize, kind: u32) -> i32;
        fn GetProcessHeap() -> *mut c_void;
        fn HeapAlloc(heap: *mut c_void, flags: u32, size: usize) -> *mut c_void;
        fn HeapFree(heap: *mut c_void, flags: u32, p: *mut c_void) -> i32;
    }

    // The C runtime routines the cipher code imports.
    unsafe extern "win64" fn c_memcpy(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
        std::ptr::copy_nonoverlapping(s, d, n);
        d
    }
    unsafe extern "win64" fn c_memmove(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
        std::ptr::copy(s, d, n);
        d
    }
    unsafe extern "win64" fn c_memset(d: *mut u8, v: i32, n: usize) -> *mut u8 {
        std::ptr::write_bytes(d, v as u8, n);
        d
    }
    unsafe extern "win64" fn c_memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
        for i in 0..n {
            let (x, y) = (*a.add(i), *b.add(i));
            if x != y {
                return x as i32 - y as i32;
            }
        }
        0
    }
    unsafe extern "win64" fn c_malloc(n: usize) -> *mut c_void {
        HeapAlloc(GetProcessHeap(), 0, n.max(1))
    }
    unsafe extern "win64" fn c_calloc(n: usize, size: usize) -> *mut c_void {
        match n.checked_mul(size) {
            Some(t) => HeapAlloc(GetProcessHeap(), HEAP_ZERO_MEMORY, t.max(1)),
            None => std::ptr::null_mut(),
        }
    }
    unsafe extern "win64" fn c_free(p: *mut c_void) {
        if !p.is_null() {
            HeapFree(GetProcessHeap(), 0, p);
        }
    }
    fn runtime(name: &[u8]) -> Option<usize> {
        Some(match name {
            b"memcpy" => c_memcpy as *const () as usize,
            b"memmove" => c_memmove as *const () as usize,
            b"memset" => c_memset as *const () as usize,
            b"memcmp" => c_memcmp as *const () as usize,
            b"malloc" => c_malloc as *const () as usize,
            b"calloc" => c_calloc as *const () as usize,
            b"free" => c_free as *const () as usize,
            _ => return None,
        })
    }

    type BlockSetKey = unsafe extern "win64" fn(*const u8, *const u8, *mut usize) -> u32;
    type BlockSetIv = unsafe extern "win64" fn(usize, *const u8) -> u32;
    type StreamSetKey = unsafe extern "win64" fn(*const u8, *const u8, usize, *mut usize) -> u32;
    type StreamSetIv = unsafe extern "win64" fn(usize, *const u8, usize) -> u32;
    type Decrypt = unsafe extern "win64" fn(usize, *const u8, usize, *mut u8) -> u32;

    fn u16le(b: &[u8], o: usize) -> Option<usize> {
        Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?) as usize)
    }
    fn u32le(b: &[u8], o: usize) -> Option<usize> {
        Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize)
    }
    fn u64le(b: &[u8], o: usize) -> Option<u64> {
        Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
    }

    struct Pe<'a> {
        file: &'a [u8],
        opt: usize,
        image_base: u64,
        image_size: usize,
        stamp: u32,
        /// (virtual address, virtual size, file offset, file size)
        sections: Vec<(usize, usize, usize, usize)>,
    }

    impl<'a> Pe<'a> {
        fn parse(file: &'a [u8]) -> Option<Pe<'a>> {
            let pe = u32le(file, 0x3c)?;
            if file.get(..2)? != b"MZ" || file.get(pe..pe + 4)? != b"PE\0\0" || u16le(file, pe + 4)? != 0x8664 {
                return None;
            }
            let opt = pe + 24;
            if u16le(file, opt)? != 0x20b {
                return None;
            }
            let table = opt + u16le(file, pe + 20)?;
            let sections = (0..u16le(file, pe + 6)?)
                .map(|i| {
                    let o = table + i * 40;
                    Some((u32le(file, o + 12)?, u32le(file, o + 8)?, u32le(file, o + 20)?, u32le(file, o + 16)?))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Pe { file, opt, image_base: u64le(file, opt + 24)?, image_size: u32le(file, opt + 56)?, stamp: u32le(file, pe + 8)? as u32, sections })
        }
        /// File offset of a relative virtual address.
        fn off(&self, rva: usize) -> Option<usize> {
            self.sections.iter().find(|s| s.0 <= rva && rva < s.0 + s.3).map(|s| s.2 + rva - s.0)
        }
        fn dir(&self, i: usize) -> Option<(usize, usize)> {
            Some((u32le(self.file, self.opt + 112 + i * 8)?, u32le(self.file, self.opt + 116 + i * 8)?))
        }
    }

    pub struct GameCipher {
        base: *mut u8,
        size: usize,
        trap: *mut c_void,
        /// key id -> (address of the key blob in the image)
        keys: HashMap<u32, usize>,
        stream_ctx: HashMap<u32, usize>,
    }

    impl Drop for GameCipher {
        fn drop(&mut self) {
            // Cipher contexts live on the process heap and are left to it.
            unsafe {
                VirtualFree(self.base.cast(), 0, MEM_RELEASE);
                VirtualFree(self.trap, 0, MEM_RELEASE);
            }
        }
    }

    /// Whether the file at `path` is the supported game executable, judged by
    /// its header alone.
    fn is_supported_build(path: &Path) -> bool {
        use std::io::Read;
        let mut head = vec![0u8; 0x1000];
        let Ok(n) = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)) else { return false };
        head.truncate(n);
        // Sections may lie past the first page; only the fixed fields are needed.
        let Some(pe) = u32le(&head, 0x3c) else { return false };
        head.get(pe..pe + 4) == Some(b"PE\0\0") && u32le(&head, pe + 8) == Some(BUILD_STAMP as usize) && u32le(&head, pe + 24 + 56) == Some(BUILD_IMAGE_SIZE as usize)
    }

    pub fn open(game: &Path) -> Result<Box<dyn ArchiveCipher>> {
        let mut exes: Vec<_> = std::fs::read_dir(game)
            .with_context(|| format!("read {}", game.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")))
            .collect();
        exes.sort();
        let Some(exe) = exes.iter().find(|p| is_supported_build(p)) else {
            bail!(
                "no supported game executable in {}: this version of SixNG works with one game build only (found {} .exe file(s), none of that build)",
                game.display(),
                exes.len()
            );
        };
        Ok(Box::new(GameCipher::load(exe)?))
    }

    impl GameCipher {
        pub fn load(exe: &Path) -> Result<GameCipher> {
            ensure!(std::arch::is_x86_feature_detected!("avx2"), "reading the table of contents needs a CPU with AVX2");
            let file = std::fs::read(exe).with_context(|| format!("read {}", exe.display()))?;
            let pe = Pe::parse(&file).ok_or_else(|| anyhow!("{}: not a 64-bit Windows executable", exe.display()))?;
            ensure!(
                pe.stamp == BUILD_STAMP && pe.image_size == BUILD_IMAGE_SIZE as usize,
                "{}: a different game build than the one this version of SixNG supports",
                exe.display()
            );
            let bad = || anyhow!("{}: the executable is damaged", exe.display());
            let size = pe.sections.iter().map(|s| s.0 + s.1.max(s.3)).max().unwrap_or(0).max(pe.image_size);
            let size = (size + 0xffff) & !0xffff;
            for rva in [KEY_LIST + KEY_COUNT * 24, BLOCK_SETKEY, STREAM_SETKEY, GAME_FREE] {
                ensure!(rva < size, bad());
            }
            // SAFETY: the image is copied into fresh memory of its full size;
            // every write below is bounds-checked against `size`.
            unsafe {
                let mut base = VirtualAlloc(pe.image_base as *mut c_void, size, MEM_COMMIT_RESERVE, PAGE_EXECUTE_READWRITE);
                if base.is_null() {
                    base = VirtualAlloc(std::ptr::null_mut(), size, MEM_COMMIT_RESERVE, PAGE_EXECUTE_READWRITE);
                }
                ensure!(!base.is_null(), "out of memory mapping the game executable ({size:#x} bytes)");
                let trap = VirtualAlloc(std::ptr::null_mut(), 0x1000, MEM_COMMIT_RESERVE, PAGE_EXECUTE_READWRITE);
                // From here on `me` frees both on any error.
                let mut me = GameCipher { base: base.cast(), size, trap, keys: HashMap::new(), stream_ctx: HashMap::new() };
                ensure!(!trap.is_null(), "out of memory");
                std::ptr::write_bytes(trap.cast::<u8>(), 0xcc, 0x1000);
                let image = std::slice::from_raw_parts_mut(me.base, size);
                for &(va, _, ro, rs) in &pe.sections {
                    let src = file.get(ro..ro + rs).ok_or_else(bad)?;
                    image.get_mut(va..va + rs).ok_or_else(bad)?.copy_from_slice(src);
                }
                let put = |image: &mut [u8], at: usize, v: u64| -> Result<()> {
                    image.get_mut(at..at + 8).ok_or_else(bad)?.copy_from_slice(&v.to_le_bytes());
                    Ok(())
                };
                let delta = (me.base as u64).wrapping_sub(pe.image_base);
                if delta != 0 {
                    let (rva, len) = pe.dir(5).ok_or_else(bad)?;
                    ensure!(rva != 0, "{}: cannot be loaded at another address", exe.display());
                    let mut o = pe.off(rva).ok_or_else(bad)?;
                    let end = o + len;
                    while o + 8 <= end {
                        let (page, block) = (u32le(&file, o).ok_or_else(bad)?, u32le(&file, o + 4).ok_or_else(bad)?);
                        if block < 8 {
                            break;
                        }
                        for j in 0..(block - 8) / 2 {
                            let e = u16le(&file, o + 8 + j * 2).ok_or_else(bad)?;
                            if e >> 12 == 10 {
                                let at = page + (e & 0xfff);
                                let v = u64le(image, at).ok_or_else(bad)?;
                                put(image, at, v.wrapping_add(delta))?;
                            }
                        }
                        o += block;
                    }
                }
                // Imports: the memory routines above, a trap for the rest.
                let (imports, _) = pe.dir(1).ok_or_else(bad)?;
                let mut o = pe.off(imports).ok_or_else(bad)?;
                loop {
                    let (lookup, name, thunks) = (u32le(&file, o).ok_or_else(bad)?, u32le(&file, o + 12).ok_or_else(bad)?, u32le(&file, o + 16).ok_or_else(bad)?);
                    if name == 0 {
                        break;
                    }
                    let names = pe.off(if lookup != 0 { lookup } else { thunks }).ok_or_else(bad)?;
                    for i in 0.. {
                        let v = u64le(&file, names + i * 8).ok_or_else(bad)?;
                        if v == 0 {
                            break;
                        }
                        let target = if v >> 63 == 0 {
                            let s = pe.off((v & 0x7fff_ffff) as usize + 2).ok_or_else(bad)?;
                            let n = file[s..].iter().position(|&c| c == 0).ok_or_else(bad)?;
                            runtime(&file[s..s + n])
                        } else {
                            None
                        };
                        put(image, thunks + i * 8, target.unwrap_or(trap as usize) as u64)?;
                    }
                    o += 20;
                }
                // jmp [rip+0]; <address>
                for (rva, target) in [(GAME_ALLOC, c_malloc as *const () as usize), (GAME_FREE, c_free as *const () as usize)] {
                    let code = image.get_mut(rva..rva + 14).ok_or_else(bad)?;
                    code[..6].copy_from_slice(&[0xff, 0x25, 0, 0, 0, 0]);
                    code[6..].copy_from_slice(&(target as u64).to_le_bytes());
                }
                // The key list holds addresses; they are read from the file, where
                // they are relative to the preferred base.
                let list = pe.off(KEY_LIST).ok_or_else(bad)?;
                for i in 0..KEY_COUNT {
                    let o = list + i * 24;
                    let (k, p, n) = (u64le(&file, o).ok_or_else(bad)?, u64le(&file, o + 8).ok_or_else(bad)?, u64le(&file, o + 16).ok_or_else(bad)?);
                    let rva = p.wrapping_sub(pe.image_base);
                    ensure!(rva.checked_add(n).is_some_and(|e| e <= size as u64), bad());
                    if (k >> 24) as u8 == KEY_FLAG {
                        me.keys.insert((k >> 32) as u32, rva as usize);
                    }
                }
                Ok(me)
            }
        }

        fn at(&self, rva: usize) -> *const u8 {
            debug_assert!(rva < self.size);
            // SAFETY: every caller passes one of the constants above, all checked in `load`.
            unsafe { self.base.add(rva) }
        }
        fn key(&self, key_id: u32) -> Result<*const u8> {
            self.keys.get(&key_id).map(|&rva| self.at(rva)).ok_or_else(|| anyhow!("key id {key_id:08x} is not in the game executable"))
        }
        fn block_setkey(&mut self, key_id: u32, iv: &[u8; 16]) -> Result<usize> {
            let blob = self.key(key_id)?;
            let mut ctx = 0usize;
            // SAFETY: calls into the mapped image with the signature found for this build.
            let r = unsafe { std::mem::transmute::<*const u8, BlockSetKey>(self.at(BLOCK_SETKEY))(blob, iv.as_ptr(), &mut ctx) };
            ensure!(r == OK && ctx != 0, "the game refused key id {key_id:08x} ({r:08x})");
            Ok(ctx)
        }
    }

    impl ArchiveCipher for GameCipher {
        fn header_key(&mut self, selector: u8) -> Result<usize> {
            let &(_, key_id, iv_rva) =
                HEADER_KEYS.iter().find(|k| k.0 == selector).ok_or_else(|| anyhow!("header key selector {selector} is not supported"))?;
            // SAFETY: the address is inside the image (checked against KEY_LIST, which lies above it).
            let iv: [u8; 16] = unsafe { std::ptr::read_unaligned(self.at(iv_rva).cast()) };
            self.block_setkey(key_id, &iv)
        }
        fn block_key(&mut self, key_id: u32, iv: &[u8; 16]) -> Result<usize> {
            self.block_setkey(key_id, iv)
        }
        fn block_reset(&mut self, ctx: usize, iv: &[u8; 16]) -> Result<()> {
            // SAFETY: as in block_setkey; `ctx` came from it.
            let r = unsafe { std::mem::transmute::<*const u8, BlockSetIv>(self.at(BLOCK_SETIV))(ctx, iv.as_ptr()) };
            ensure!(r == OK, "the game's cipher failed to restart ({r:08x})");
            Ok(())
        }
        fn block_decrypt(&mut self, ctx: usize, buf: &mut [u8]) -> Result<()> {
            let n = buf.len() & !15;
            if n == 0 {
                return Ok(());
            }
            let mut out = vec![0u8; n];
            // SAFETY: as in block_setkey; both buffers hold `n` bytes.
            let r = unsafe { std::mem::transmute::<*const u8, Decrypt>(self.at(BLOCK_DECRYPT))(ctx, buf.as_ptr(), n, out.as_mut_ptr()) };
            ensure!(r == OK, "the game's cipher failed ({r:08x})");
            buf[..n].copy_from_slice(&out);
            Ok(())
        }
        fn stream_decrypt(&mut self, key_id: u32, iv: &[u8; 16], buf: &mut [u8]) -> Result<()> {
            if buf.is_empty() {
                return Ok(());
            }
            let ctx = match self.stream_ctx.get(&key_id) {
                Some(&ctx) => {
                    // SAFETY: as in block_setkey; `ctx` came from the stream set-key routine.
                    let r = unsafe { std::mem::transmute::<*const u8, StreamSetIv>(self.at(STREAM_SETIV))(ctx, iv.as_ptr(), 16) };
                    ensure!(r == OK, "the game's cipher failed to restart ({r:08x})");
                    ctx
                }
                None => {
                    let blob = self.key(key_id)?;
                    let mut ctx = 0usize;
                    // SAFETY: as in block_setkey.
                    let r = unsafe { std::mem::transmute::<*const u8, StreamSetKey>(self.at(STREAM_SETKEY))(blob, iv.as_ptr(), 16, &mut ctx) };
                    ensure!(r == OK && ctx != 0, "the game refused key id {key_id:08x} ({r:08x})");
                    self.stream_ctx.insert(key_id, ctx);
                    ctx
                }
            };
            let mut out = vec![0u8; buf.len()];
            // SAFETY: as in block_setkey; both buffers hold `buf.len()` bytes.
            let r = unsafe { std::mem::transmute::<*const u8, Decrypt>(self.at(STREAM_DECRYPT))(ctx, buf.as_ptr(), buf.len(), out.as_mut_ptr()) };
            ensure!(r == OK, "the game's cipher failed ({r:08x})");
            buf.copy_from_slice(&out);
            Ok(())
        }
    }
}
