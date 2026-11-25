//! Diagnostics: what a dispatch of one stage of the ray-tracing structure
//! builder leaves in title memory, saved to a file for checking off line.
//!
//! `VICTORY_BVH_DUMP=<k>` (the stage's number, as `pipeline::builder_dry`
//! logs it) and `VICTORY_BVH_DUMP_DIR=<folder>`: for the first
//! [`MOST`] dispatches of stage k, once their list has run (its shaders'
//! writes are back in title memory), a file `stage<k>-<n>.bin` in the folder
//! holds, as little-endian fields:
//!
//! - `"VBVD"`, a format number (2), the dispatch's group counts (3 × u32);
//! - the constants of the root parameter the stage's first user data word
//!   is in (a count, then the dwords);
//! - its root views (a count, then per view the root slot as u32 and the
//!   title address as u64);
//! - pieces of title memory (a count, then per piece the address as u64, the
//!   bytes as u32 and the bytes). Per group g of the dispatch (at most
//!   [`GROUPS`]): [`BLOCK`] bytes at every root view + constant 0 + g × 4 KiB; then
//!   128 bytes at every root view + (r × 8 & !63) for each r of the first
//!   four dwords of every 64 bytes of those pieces that lies in the memory
//!   (the nodes they name, if they are node references).
//!
//! - format 2: the same blocks as title memory held them when the dispatch
//!   was recorded (a count, then as the pieces).
//!
//! Memory that is not committed is left out. Nothing is read or written
//! but title memory; the files stay where the switch says.

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::Mutex;

/// Dispatches dumped per run.
const MOST: u32 = 8;
/// Groups of a dispatch whose blocks are dumped.
const GROUPS: u32 = 16;
/// Pieces of title memory per file, at most.
const PIECES: usize = 8192;

struct Asked {
    stage: usize,
    groups: [u32; 3],
    constants: Vec<u32>,
    views: Vec<(u8, u64)>,
    /// The blocks as title memory held them when the dispatch was recorded
    /// (before its submission: what earlier dispatches of the list write
    /// is not in them).
    before: Vec<(u64, Vec<u8>)>,
}

static ASKED: Mutex<Option<HashMap<u32, Asked>>> = Mutex::new(None);
static TAKEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The stage to dump, if any.
fn stage() -> Option<usize> {
    static STAGE: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    *STAGE.get_or_init(|| std::env::var("VICTORY_BVH_DUMP").ok().and_then(|v| v.trim().parse().ok()))
}

fn folder() -> std::path::PathBuf {
    std::env::var_os("VICTORY_BVH_DUMP_DIR").map_or_else(|| std::path::PathBuf::from("."), Into::into)
}

/// A dispatch of builder stage `at` is about to be recorded: its number if
/// it is to be dumped once its list has run.
pub(crate) fn ask(at: usize, groups: [u32; 3], constants: Vec<u32>, views: Vec<(u8, u64)>) -> Option<u32> {
    if stage() != Some(at) {
        return None;
    }
    let n = TAKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n >= MOST {
        return None;
    }
    tracing::info!(
        stage = at,
        n,
        ?groups,
        constants = format_args!("{constants:x?}"),
        views = format_args!("{views:x?}"),
        "builder dump: a dispatch to save once its list has run"
    );
    let before = blocks(groups, &constants, &views);
    ASKED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(n, Asked { stage: at, groups, constants, views, before });
    Some(n)
}

/// Title memory `[at, at + len)`, if committed.
fn read(at: u64, len: usize) -> Option<Vec<u8>> {
    if at == 0 || !crate::texture::committed(at as *const u8, len) {
        return None;
    }
    let mut bytes = vec![0u8; len];
    // SAFETY: committed title memory; the list that wrote it has run.
    unsafe { std::ptr::copy_nonoverlapping(at as *const u8, bytes.as_mut_ptr(), len) };
    Some(bytes)
}

/// Per group of the dispatch (at most [`GROUPS`]), [`BLOCK`] bytes at every
/// root view + constant 0 + g × 4 KiB, where committed.
fn blocks(groups: [u32; 3], constants: &[u32], views: &[(u8, u64)]) -> Vec<(u64, Vec<u8>)> {
    let base = u64::from(constants.first().copied().unwrap_or(0));
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for g in 0..groups[0].min(GROUPS) {
        for &(_, view) in views {
            let at = view + base + u64::from(g) * 4096;
            if seen.insert(at) && let Some(bytes) = read(at, BLOCK) {
                out.push((at, bytes));
            }
        }
    }
    out
}

/// Bytes of a block: two groups' worth, for stages whose lanes write
/// 128 bytes each.
const BLOCK: usize = 8192;

/// Dispatch `n`'s list has run: its file.
pub(crate) fn after(n: u32) {
    let Some(asked) = ASKED.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|m| m.remove(&n)) else {
        return;
    };
    let mut pieces = blocks(asked.groups, &asked.constants, &asked.views);
    let mut seen: std::collections::HashSet<u64> = pieces.iter().map(|p| p.0).collect();
    let blocks = pieces.len();
    for i in 0..blocks {
        let refs: Vec<u32> = pieces[i]
            .1
            .chunks_exact(64)
            .flat_map(|node| node[..16].chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])))
            .filter(|&r| r != u32::MAX && r != 0)
            .collect();
        for r in refs {
            for &(_, view) in &asked.views {
                let at = view + ((u64::from(r) * 8) & !63);
                if pieces.len() < PIECES && seen.insert(at) && let Some(bytes) = read(at, 128) {
                    pieces.push((at, bytes));
                }
            }
        }
    }
    let path = folder().join(format!("stage{}-{n}.bin", asked.stage));
    let mut out = Vec::new();
    out.extend_from_slice(b"VBVD");
    out.extend_from_slice(&2u32.to_le_bytes());
    for g in asked.groups {
        out.extend_from_slice(&g.to_le_bytes());
    }
    out.extend_from_slice(&(asked.constants.len() as u32).to_le_bytes());
    for c in &asked.constants {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&(asked.views.len() as u32).to_le_bytes());
    for &(slot, at) in &asked.views {
        out.extend_from_slice(&u32::from(slot).to_le_bytes());
        out.extend_from_slice(&at.to_le_bytes());
    }
    out.extend_from_slice(&(pieces.len() as u32).to_le_bytes());
    for (at, bytes) in &pieces {
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    // Format 2: then the blocks as they were before (a count, then as the
    // pieces).
    out.extend_from_slice(&(asked.before.len() as u32).to_le_bytes());
    for (at, bytes) in &asked.before {
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    let written = std::fs::File::create(&path).and_then(|mut f| f.write_all(&out));
    tracing::info!(
        n,
        path = %path.display(),
        blocks,
        pieces = pieces.len(),
        ok = written.is_ok(),
        "builder dump: saved"
    );
}
