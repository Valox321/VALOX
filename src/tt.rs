//! Shared, lock-free transposition table.
//!
//! Every slot is two 8-byte words: a verification word (`key ^ data`) and the
//! packed entry. Probes are wait-free; a concurrent write can only produce a
//! mismatched verification word, so a torn entry is rejected. This lets every
//! search thread share one table without locks.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::position::{Move, Piece};

/// Size of one table slot: the verification word plus the packed entry.
pub const SLOT_BYTES: usize = 16;

const NO_EVAL: i16 = i16::MIN;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Bound {
    Exact = 0,
    Lower = 1,
    Upper = 2,
}

impl Bound {
    pub fn from_flags(flags: u8) -> Self {
        match flags & 3 {
            0 => Bound::Exact,
            1 => Bound::Lower,
            _ => Bound::Upper,
        }
    }
}

/// A transposition entry packed into one 64-bit word.
///
/// Layout: best move (16), depth (8), score (16), static eval (16), age (6),
/// bound (2). Scores and evaluations always fit in 16 bits because mate is
/// 30,000 centipawns and the UCI depth is capped far below 255.
#[derive(Clone, Copy)]
pub struct Entry(u64);

impl Entry {
    pub fn new(
        depth: i32,
        score: i32,
        bound: Bound,
        best: Option<Move>,
        age: u8,
        eval: Option<i32>,
    ) -> Self {
        let best = best.map_or(0, |mv| u64::from(encode_move(mv)));
        let depth = depth.clamp(0, 255) as u64;
        let score = score.clamp(i16::MIN as i32, i16::MAX as i32) as i16 as u16 as u64;
        let eval = eval
            .map(|value| value.clamp(i16::MIN as i32 + 1, i16::MAX as i32) as i16)
            .unwrap_or(NO_EVAL) as u16 as u64;
        let age = u64::from(age & 0x3f);
        Self(
            best | (depth << 16)
                | (score << 24)
                | (eval << 40)
                | (age << 56)
                | ((bound as u64) << 62),
        )
    }

    pub fn depth(self) -> i32 {
        ((self.0 >> 16) & 0xff) as i32
    }

    pub fn score(self) -> i32 {
        ((self.0 >> 24) & 0xffff) as u16 as i16 as i32
    }

    pub fn eval(self) -> Option<i32> {
        let eval = ((self.0 >> 40) & 0xffff) as u16 as i16;
        (eval != NO_EVAL).then_some(i32::from(eval))
    }

    pub fn age(self) -> u8 {
        ((self.0 >> 56) & 0x3f) as u8
    }

    pub fn bound(self) -> Bound {
        Bound::from_flags((self.0 >> 62) as u8)
    }

    pub fn best(self) -> Option<Move> {
        decode_move((self.0 & 0xffff) as u16)
    }

    fn raw(self) -> u64 {
        self.0
    }
}

fn promotion_code(piece: Piece) -> u16 {
    match piece {
        Piece::Knight => 1,
        Piece::Bishop => 2,
        Piece::Rook => 3,
        Piece::Queen => 4,
        Piece::Pawn | Piece::King => 0,
    }
}

fn promotion_piece(code: u16) -> Option<Piece> {
    match code {
        1 => Some(Piece::Knight),
        2 => Some(Piece::Bishop),
        3 => Some(Piece::Rook),
        4 => Some(Piece::Queen),
        _ => None,
    }
}

/// Pack a move into 16 bits. Castling stores the rook origin in the target
/// field plus a flag; the king target is derived from the rook's side, which
/// is how castling rights are defined, so the decoded move is exactly the one
/// move generation produced.
fn encode_move(mv: Move) -> u16 {
    let from = u16::from(mv.from);
    if let Some(rook) = mv.castle_rook_from {
        return from | (u16::from(rook) << 6) | 0x8000;
    }
    let to = u16::from(mv.to);
    from | (to << 6) | (mv.promotion.map_or(0, promotion_code) << 12)
}

fn decode_move(bits: u16) -> Option<Move> {
    if bits == 0 {
        return None;
    }
    let from = (bits & 0x3f) as u8;
    let second = ((bits >> 6) & 0x3f) as u8;
    if bits & 0x8000 != 0 {
        let kingside = (second & 7) > (from & 7);
        let to = (from & 56) | if kingside { 6 } else { 2 };
        Some(Move {
            from,
            to,
            promotion: None,
            castle_rook_from: Some(second),
        })
    } else {
        Some(Move {
            from,
            to: second,
            promotion: promotion_piece((bits >> 12) & 0x7),
            castle_rook_from: None,
        })
    }
}

/// Number of entries that fit a UCI `Hash` setting, rounded down to a power
/// of two so slot lookup reduces to one mask.
pub fn entries_for_mb(hash_mb: usize) -> usize {
    let entries = (hash_mb.saturating_mul(1024 * 1024) / SLOT_BYTES).max(1);
    if entries.is_power_of_two() {
        entries
    } else {
        1usize << (usize::BITS - 1 - entries.leading_zeros())
    }
}

pub struct TranspositionTable {
    slots: Vec<AtomicU64>,
    mask: u64,
}

impl TranspositionTable {
    pub fn new(entries: usize) -> Self {
        let entries = entries.max(1).next_power_of_two();
        let mut slots = Vec::with_capacity(entries * 2);
        slots.resize_with(entries * 2, || AtomicU64::new(0));
        Self {
            slots,
            mask: (entries - 1) as u64,
        }
    }

    pub fn entries(&self) -> usize {
        self.slots.len() / 2
    }

    pub fn clear(&self) {
        for slot in &self.slots {
            slot.store(0, Ordering::Relaxed);
        }
    }

    pub fn probe(&self, key: u64) -> Option<Entry> {
        let index = (key & self.mask) as usize * 2;
        let check = self.slots[index].load(Ordering::Acquire);
        let data = self.slots[index + 1].load(Ordering::Relaxed);
        (data != 0 && check ^ data == key).then_some(Entry(data))
    }

    pub fn store(&self, key: u64, entry: Entry) {
        let index = (key & self.mask) as usize * 2;
        let check = self.slots[index].load(Ordering::Acquire);
        let data = self.slots[index + 1].load(Ordering::Relaxed);
        let old = Entry(data);
        let replace = if data == 0 {
            true
        } else if check ^ data == key {
            entry.depth() >= old.depth() || entry.bound() == Bound::Exact
        } else {
            // A different position owns the slot. Keep deep results, but let
            // younger searches age them out so the table does not freeze on
            // the first deep entries it sees.
            let age = entry.age().wrapping_sub(old.age()) & 0x3f;
            entry.depth() + 2 * i32::from(age) >= old.depth()
        };
        if replace {
            // Publish the payload first; the verification word is the
            // release that makes the pair observable as a unit.
            self.slots[index + 1].store(entry.raw(), Ordering::Relaxed);
            self.slots[index].store(key ^ entry.raw(), Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_move, encode_move, entries_for_mb, Bound, Entry, TranspositionTable, SLOT_BYTES,
    };
    use crate::position::{Move, Piece};
    use std::sync::atomic::AtomicU64;

    fn mv(from: u8, to: u8) -> Move {
        Move {
            from,
            to,
            promotion: None,
            castle_rook_from: None,
        }
    }

    #[test]
    fn entry_roundtrips_all_fields() {
        let best = Move {
            from: 12,
            to: 28,
            promotion: Some(Piece::Queen),
            castle_rook_from: None,
        };
        let entry = Entry::new(23, -1234, Bound::Lower, Some(best), 42, Some(-321));
        assert_eq!(entry.depth(), 23);
        assert_eq!(entry.score(), -1234);
        assert_eq!(entry.best(), Some(best));
        assert_eq!(entry.age(), 42);
        assert_eq!(entry.eval(), Some(-321));
        assert_eq!(entry.bound(), Bound::Lower);
    }

    #[test]
    fn entry_without_eval_or_move_roundtrips() {
        let entry = Entry::new(1, 50, Bound::Upper, None, 3, None);
        assert_eq!(entry.best(), None);
        assert_eq!(entry.eval(), None);
        assert_eq!(entry.bound(), Bound::Upper);
    }

    #[test]
    fn castling_moves_roundtrip() {
        let standard = [
            Move {
                from: 4,
                to: 6,
                promotion: None,
                castle_rook_from: Some(7),
            },
            Move {
                from: 4,
                to: 2,
                promotion: None,
                castle_rook_from: Some(0),
            },
            Move {
                from: 60,
                to: 62,
                promotion: None,
                castle_rook_from: Some(63),
            },
            Move {
                from: 60,
                to: 58,
                promotion: None,
                castle_rook_from: Some(56),
            },
        ];
        for mv in standard {
            assert_eq!(decode_move(encode_move(mv)), Some(mv));
        }
        // Chess960: king on b1, rook on d1 and on a1.
        let chess960 = [
            Move {
                from: 1,
                to: 6,
                promotion: None,
                castle_rook_from: Some(3),
            },
            Move {
                from: 1,
                to: 2,
                promotion: None,
                castle_rook_from: Some(0),
            },
            Move {
                from: 2,
                to: 2,
                promotion: None,
                castle_rook_from: Some(0),
            },
        ];
        for mv in chess960 {
            assert_eq!(decode_move(encode_move(mv)), Some(mv));
        }
    }

    #[test]
    fn ordinary_moves_roundtrip() {
        let moves = [
            mv(0, 63),
            mv(52, 44),
            Move {
                from: 48,
                to: 56,
                promotion: Some(Piece::Rook),
                castle_rook_from: None,
            },
        ];
        for mv in moves {
            assert_eq!(decode_move(encode_move(mv)), Some(mv));
        }
    }

    #[test]
    fn table_probes_only_matching_keys() {
        let table = TranspositionTable::new(1024);
        let entry = Entry::new(8, 77, Bound::Exact, Some(mv(12, 28)), 1, Some(10));
        table.store(0x1234_5678, entry);
        let found = table.probe(0x1234_5678).expect("entry must be visible");
        assert_eq!(found.depth(), 8);
        assert_eq!(found.score(), 77);
        assert!(table.probe(0x1234_5679).is_none());
        table.clear();
        assert!(table.probe(0x1234_5678).is_none());
    }

    #[test]
    fn slots_stay_sixteen_bytes() {
        assert_eq!(SLOT_BYTES, 16);
        assert_eq!(std::mem::size_of::<Entry>(), 8);
        assert_eq!(std::mem::size_of::<[AtomicU64; 2]>(), SLOT_BYTES);
    }

    #[test]
    fn hash_entries_scale_with_megabytes() {
        assert_eq!(entries_for_mb(16), 1_048_576);
        assert_eq!(entries_for_mb(16) * SLOT_BYTES, 16 * 1024 * 1024);
        assert_eq!(entries_for_mb(256) * SLOT_BYTES, 256 * 1024 * 1024);
        assert_eq!(entries_for_mb(0), 1);
    }
}
