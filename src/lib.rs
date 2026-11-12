pub mod engine;
pub mod nnue;
pub mod position;
#[cfg(not(target_arch = "wasm32"))]
pub mod syzygy;

use position::{parse_uci_move, Position};
use std::alloc::{alloc, dealloc, Layout};
use std::ptr;
use std::slice;
use std::str;

#[no_mangle]
pub extern "C" fn valox_alloc(size: usize) -> *mut u8 {
    let Ok(layout) = Layout::array::<u8>(size.max(1)) else {
        return ptr::null_mut();
    };
    unsafe { alloc(layout) }
}

#[no_mangle]
/// # Safety
/// `pointer` must be null or the exact pointer returned by `valox_alloc`, and `size`
/// must be the same size passed to that allocation call.
pub unsafe extern "C" fn valox_dealloc(pointer: *mut u8, size: usize) {
    if pointer.is_null() {
        return;
    }
    if let Ok(layout) = Layout::array::<u8>(size.max(1)) {
        dealloc(pointer, layout);
    }
}

unsafe fn input<'a>(pointer: *const u8, length: usize) -> Option<&'a str> {
    if pointer.is_null() {
        return None;
    }
    str::from_utf8(slice::from_raw_parts(pointer, length)).ok()
}

unsafe fn output(bytes: &[u8], pointer: *mut u8, capacity: usize) -> usize {
    if pointer.is_null() || bytes.len() > capacity {
        return 0;
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
    bytes.len()
}

#[no_mangle]
/// # Safety
/// `fen` must point to `fen_len` readable bytes. `out` must point to `capacity`
/// writable bytes, or be null only when the output capacity is zero.
pub unsafe extern "C" fn valox_legal_moves(
    fen: *const u8,
    fen_len: usize,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let Some(fen) = input(fen, fen_len) else {
        return 0;
    };
    let Some(position) = Position::from_fen(fen) else {
        return 0;
    };
    let moves = position
        .legal_moves()
        .iter()
        .map(|mv| mv.uci())
        .collect::<Vec<_>>()
        .join(" ");
    output(moves.as_bytes(), out, capacity)
}

#[no_mangle]
/// # Safety
/// `fen` and `mv` must point to their respective readable byte lengths. `out` must
/// point to `capacity` writable bytes, or be null only when the output capacity is zero.
pub unsafe extern "C" fn valox_apply_move(
    fen: *const u8,
    fen_len: usize,
    mv: *const u8,
    mv_len: usize,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let Some(fen) = input(fen, fen_len) else {
        return 0;
    };
    let Some(move_text) = input(mv, mv_len) else {
        return 0;
    };
    let Some(position) = Position::from_fen(fen) else {
        return 0;
    };
    let Some(move_value) = parse_uci_move(&position, move_text) else {
        return 0;
    };
    output(position.play(move_value).to_fen().as_bytes(), out, capacity)
}

#[no_mangle]
/// # Safety
/// `fen` must point to `fen_len` readable bytes. `out` must point to `capacity`
/// writable bytes, or be null only when the output capacity is zero.
pub unsafe extern "C" fn valox_best_move(
    fen: *const u8,
    fen_len: usize,
    depth: u32,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let Some(fen) = input(fen, fen_len) else {
        return 0;
    };
    let Some(position) = Position::from_fen(fen) else {
        return 0;
    };
    let Some(best) = engine::best_move(&position, depth.clamp(1, 64)) else {
        return 0;
    };
    output(best.uci().as_bytes(), out, capacity)
}

#[no_mangle]
/// # Safety
/// `fen` must point to `fen_len` readable bytes.
pub unsafe extern "C" fn valox_status(fen: *const u8, fen_len: usize) -> u32 {
    let Some(fen) = input(fen, fen_len) else {
        return 4;
    };
    let Some(position) = Position::from_fen(fen) else {
        return 4;
    };
    let legal = position.legal_moves();
    if legal.is_empty() && position.in_check(position.side) {
        return 1;
    }
    if legal.is_empty() {
        return 2;
    }
    if position.is_fifty_move_draw() {
        return 3;
    }
    if position.is_insufficient_material() {
        return 5;
    }
    0
}
