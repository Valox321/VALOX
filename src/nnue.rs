//! Scalar inference for the locally installed Reckless v60 NNUE weights.
//! The feature extraction and forward pass are implemented in Valox.

use crate::position::{Piece, Position, Side};
use std::sync::OnceLock;

// Model weights are intentionally not tracked in the public source repository.
// A local copy is embedded when available; source-only builds use the classical evaluator.
#[cfg(valox_nnue_weights)]
const MODEL: &[u8] = include_bytes!("../networks/v60-7f587dfb.nnue");
#[cfg(not(valox_nnue_weights))]
const MODEL: &[u8] = &[];
const FEATURES: usize = 768;
const THREAT_FEATURES: usize = 66_864;
const INPUT_BUCKETS: usize = 10;
const OUTPUT_BUCKETS: usize = 8;
const L2: usize = 16;
const L3: usize = 32;
const SCALE: f32 = 512.0 / (255.0 * 255.0 * 64.0);

const THREAT_BYTES: usize = THREAT_FEATURES * FEATURES;
const PIECE_OFFSET: usize = THREAT_BYTES;
const PIECE_BYTES: usize = INPUT_BUCKETS * FEATURES * FEATURES * 2;
const BIAS_OFFSET: usize = PIECE_OFFSET + PIECE_BYTES;
const L1_OFFSET: usize = BIAS_OFFSET + FEATURES * 2;
const L1_BYTES: usize = OUTPUT_BUCKETS * L2 * FEATURES;
const L1_BIAS_OFFSET: usize = L1_OFFSET + L1_BYTES;
const L2_WEIGHT_OFFSET: usize = L1_BIAS_OFFSET + OUTPUT_BUCKETS * L2 * 4;
const L2_BIAS_OFFSET: usize = L2_WEIGHT_OFFSET + OUTPUT_BUCKETS * L2 * L3 * 4;
const L3_WEIGHT_OFFSET: usize = L2_BIAS_OFFSET + OUTPUT_BUCKETS * L3 * 4;
const L3_BIAS_OFFSET: usize = L3_WEIGHT_OFFSET + OUTPUT_BUCKETS * L3 * 4;
// The serialized final layer is padded to the model's 64-byte alignment.
const MODEL_BYTES: usize = L3_BIAS_OFFSET + 64;

const PIECE_TYPES: [Piece; 6] = [
    Piece::Pawn,
    Piece::Knight,
    Piece::Bishop,
    Piece::Rook,
    Piece::Queen,
    Piece::King,
];
const TARGET_MAP: [[i8; 6]; 6] = [
    [0, 1, -1, 2, -1, -1],
    [0, 1, 2, 3, 4, -1],
    [0, 1, 2, 3, -1, -1],
    [0, 1, 2, 3, -1, -1],
    [0, 1, 2, 3, 4, -1],
    [0, 1, 2, 3, -1, -1],
];
const TARGET_COUNTS: [usize; 6] = [6, 10, 8, 8, 10, 8];
const INPUT_BUCKET_LAYOUT: [usize; 64] = [
    0, 1, 2, 3, 3, 2, 1, 0, 4, 5, 6, 7, 7, 6, 5, 4, 8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 9,
    9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
];
const OUTPUT_BUCKET_LAYOUT: [usize; 33] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 7, 7, 7,
    7,
];

struct ThreatLayout {
    base: [usize; 16],
    per_square: [[usize; 64]; 16],
    span: [usize; 16],
}

fn square_attacks(piece: Piece, square: u8, side: usize) -> [bool; 64] {
    let mut result = [false; 64];
    let file = (square % 8) as i8;
    let rank = (square / 8) as i8;
    for target in 0..64u8 {
        let df = (target % 8) as i8 - file;
        let dr = (target / 8) as i8 - rank;
        result[target as usize] = match piece {
            Piece::Pawn => {
                rank > 0 && rank < 7 && dr == if side == 0 { 1 } else { -1 } && df.abs() == 1
            }
            Piece::Knight => (df.abs() == 1 && dr.abs() == 2) || (df.abs() == 2 && dr.abs() == 1),
            Piece::Bishop => df != 0 && df.abs() == dr.abs(),
            Piece::Rook => (df == 0) != (dr == 0),
            Piece::Queen => (df != 0 && df.abs() == dr.abs()) || ((df == 0) != (dr == 0)),
            Piece::King => df.abs() <= 1 && dr.abs() <= 1 && (df != 0 || dr != 0),
        };
    }
    result
}

fn piece_code(piece: Piece, side: Side) -> usize {
    let kind = match piece {
        Piece::Pawn => 0,
        Piece::Knight => 1,
        Piece::Bishop => 2,
        Piece::Rook => 3,
        Piece::Queen => 4,
        Piece::King => 5,
    };
    kind + 1 + if side == Side::White { 0 } else { 8 }
}

fn code_type(code: usize) -> usize {
    code & 7
}

fn code_side(code: usize) -> usize {
    (code >> 3) & 1
}

fn threat_layout() -> &'static ThreatLayout {
    static LAYOUT: OnceLock<ThreatLayout> = OnceLock::new();
    LAYOUT.get_or_init(|| {
        let mut layout = ThreatLayout {
            base: [0; 16],
            per_square: [[0; 64]; 16],
            span: [0; 16],
        };
        let mut total = 0usize;
        for code in [1usize, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 14] {
            let kind = code_type(code) - 1;
            layout.base[code] = total;
            let piece = PIECE_TYPES.get(kind).copied();
            let mut count = 0;
            if let Some(piece) = piece {
                for sq in 0..64u8 {
                    layout.per_square[code][sq as usize] = count;
                    count += square_attacks(piece, sq, code_side(code))
                        .into_iter()
                        .filter(|x| *x)
                        .count();
                }
                layout.span[code] = count;
                total += TARGET_COUNTS[kind] * count;
            }
        }
        assert_eq!(
            total, THREAT_FEATURES,
            "NNUE threat feature layout does not match the embedded network"
        );
        layout
    })
}

fn king_orientation(square: u8) -> u8 {
    if square % 8 >= 4 {
        7
    } else {
        0
    }
}

fn threat_index(
    attacker: usize,
    from: u8,
    to: u8,
    attacked: usize,
    perspective: usize,
    king: u8,
) -> Option<usize> {
    let orient = king_orientation(king) ^ (56 * perspective as u8);
    let from = (from ^ orient) as usize;
    let to = (to ^ orient) as usize;
    let attacker = attacker ^ (8 * perspective);
    let attacked = attacked ^ (8 * perspective);
    let attacker_type = code_type(attacker).checked_sub(1)?;
    let attacked_type = code_type(attacked).checked_sub(1)?;
    if attacker_type >= 6 || attacked_type >= 6 {
        return None;
    }
    let map = TARGET_MAP[attacker_type][attacked_type];
    if map < 0 {
        return None;
    }
    let enemy_same_type = (attacker ^ attacked) == 8;
    let semi_excluded = attacker_type == attacked_type && (enemy_same_type || attacker_type != 0);
    if from < to && semi_excluded {
        return None;
    }
    let layout = threat_layout();
    let attacks = square_attacks(PIECE_TYPES[attacker_type], from as u8, code_side(attacker));
    if !attacks[to] {
        return None;
    }
    let square_rank = attacks[..to].iter().filter(|attacks| **attacks).count();
    let target_side = code_side(attacked);
    let within_type =
        (target_side * (TARGET_COUNTS[attacker_type] / 2) + map as usize) * layout.span[attacker];
    let index =
        layout.base[attacker] + within_type + layout.per_square[attacker][from] + square_rank;
    (index < THREAT_FEATURES).then_some(index)
}

fn model_i8(offset: usize) -> i32 {
    MODEL[offset] as i8 as i32
}

fn model_i16(offset: usize) -> i32 {
    i16::from_le_bytes([MODEL[offset], MODEL[offset + 1]]) as i32
}

fn model_f32(offset: usize) -> f32 {
    f32::from_le_bytes([
        MODEL[offset],
        MODEL[offset + 1],
        MODEL[offset + 2],
        MODEL[offset + 3],
    ])
}

fn piece_feature_indices(
    position: &Position,
    perspective: usize,
    king: u8,
    features: &mut Vec<usize>,
) {
    features.clear();
    let flip = king_orientation(king) ^ (56 * perspective as u8);
    let input_bucket = INPUT_BUCKET_LAYOUT[(king ^ flip) as usize];
    let bucket_offset = input_bucket * FEATURES;
    for square in 0..64u8 {
        let Some((piece, side)) = position.piece_at(square) else {
            continue;
        };
        let own = (side == Side::White) == (perspective == 0);
        let piece_index = match piece {
            Piece::Pawn => 0,
            Piece::Knight => 1,
            Piece::Bishop => 2,
            Piece::Rook => 3,
            Piece::Queen => 4,
            Piece::King => 5,
        };
        let feature =
            bucket_offset + usize::from(!own) * 384 + piece_index * 64 + (square ^ flip) as usize;
        features.push(feature);
    }
    features.sort_unstable();
}

fn threat_feature_indices(
    position: &Position,
    perspective: usize,
    king: u8,
    features: &mut Vec<usize>,
) {
    features.clear();
    let mut occupied = [(0u8, Piece::Pawn, Side::White); 64];
    let mut occupied_count = 0;
    for square in 0..64u8 {
        if let Some((piece, side)) = position.piece_at(square) {
            occupied[occupied_count] = (square, piece, side);
            occupied_count += 1;
        }
    }
    for &(from, attacker_piece, attacker_side) in &occupied[..occupied_count] {
        let Some(attacker_kind) = PIECE_TYPES.iter().position(|p| *p == attacker_piece) else {
            continue;
        };
        let attacker_code = piece_code(attacker_piece, attacker_side);
        for &(to, attacked_piece, attacked_side) in &occupied[..occupied_count] {
            let attacked_kind = match attacked_piece {
                Piece::Pawn => 0,
                Piece::Knight => 1,
                Piece::Bishop => 2,
                Piece::Rook => 3,
                Piece::Queen => 4,
                Piece::King => continue,
            };
            if TARGET_MAP[attacker_kind][attacked_kind] < 0
                || !position.piece_attacks_square(from, to, attacker_piece, attacker_side)
            {
                continue;
            }
            let Some(feature) = threat_index(
                attacker_code,
                from,
                to,
                piece_code(attacked_piece, attacked_side),
                perspective,
                king,
            ) else {
                continue;
            };
            features.push(feature);
        }
    }
    features.sort_unstable();
}

struct FeatureDelta {
    added: Vec<usize>,
    removed: Vec<usize>,
}

impl FeatureDelta {
    fn new(capacity: usize) -> Self {
        Self {
            added: Vec::with_capacity(capacity),
            removed: Vec::with_capacity(capacity),
        }
    }
    fn clear(&mut self) {
        self.added.clear();
        self.removed.clear();
    }
}

struct AccumulatorFrame {
    piece_delta: [FeatureDelta; 2],
    threat_delta: [FeatureDelta; 2],
    previous_pieces: [Vec<usize>; 2],
    previous_threats: [Vec<usize>; 2],
}

impl AccumulatorFrame {
    fn new() -> Self {
        Self {
            piece_delta: std::array::from_fn(|_| FeatureDelta::new(32)),
            threat_delta: std::array::from_fn(|_| FeatureDelta::new(512)),
            previous_pieces: std::array::from_fn(|_| Vec::with_capacity(32)),
            previous_threats: std::array::from_fn(|_| Vec::with_capacity(256)),
        }
    }
}

/// Search-local NNUE state. Each push applies only feature additions/removals;
/// pop reverses that delta when the search backs up the tree.
pub struct NnueEvaluator {
    accumulators: [[i32; FEATURES]; 2],
    active_pieces: [Vec<usize>; 2],
    active_threats: [Vec<usize>; 2],
    scratch_pieces: [Vec<usize>; 2],
    scratch_threats: [Vec<usize>; 2],
    frames: Vec<AccumulatorFrame>,
    depth: usize,
    ready: bool,
}

impl Default for NnueEvaluator {
    fn default() -> Self {
        Self {
            accumulators: [[0; FEATURES]; 2],
            active_pieces: std::array::from_fn(|_| Vec::with_capacity(32)),
            active_threats: std::array::from_fn(|_| Vec::with_capacity(256)),
            scratch_pieces: std::array::from_fn(|_| Vec::with_capacity(32)),
            scratch_threats: std::array::from_fn(|_| Vec::with_capacity(256)),
            frames: Vec::with_capacity(128),
            depth: 0,
            ready: false,
        }
    }
}

impl NnueEvaluator {
    pub fn reset(&mut self, position: &Position) -> bool {
        self.depth = 0;
        self.ready = false;
        if MODEL.len() != MODEL_BYTES {
            return false;
        }
        for side in 0..2 {
            self.active_pieces[side].clear();
            self.active_threats[side].clear();
            self.scratch_pieces[side].clear();
            self.scratch_threats[side].clear();
            for i in 0..FEATURES {
                self.accumulators[side][i] = model_i16(BIAS_OFFSET + i * 2);
            }
        }
        for (perspective, side) in [Side::White, Side::Black].into_iter().enumerate() {
            let Some(king) =
                (0..64u8).find(|&sq| position.piece_at(sq) == Some((Piece::King, side)))
            else {
                return false;
            };
            piece_feature_indices(
                position,
                perspective,
                king,
                &mut self.active_pieces[perspective],
            );
            threat_feature_indices(
                position,
                perspective,
                king,
                &mut self.active_threats[perspective],
            );
            apply_features(
                &mut self.accumulators[perspective],
                &self.active_pieces[perspective],
                true,
            );
            apply_features(
                &mut self.accumulators[perspective],
                &self.active_threats[perspective],
                false,
            );
        }
        self.ready = true;
        true
    }

    pub fn push(&mut self, position: &Position) {
        if !self.ready {
            return;
        }
        if self.frames.len() == self.depth {
            self.frames.push(AccumulatorFrame::new());
        }
        let frame_index = self.depth;
        let frame = &mut self.frames[frame_index];
        for perspective in 0..2 {
            frame.piece_delta[perspective].clear();
            frame.threat_delta[perspective].clear();
            let side = if perspective == 0 {
                Side::White
            } else {
                Side::Black
            };
            let Some(king) =
                (0..64u8).find(|&sq| position.piece_at(sq) == Some((Piece::King, side)))
            else {
                self.ready = false;
                return;
            };
            piece_feature_indices(
                position,
                perspective,
                king,
                &mut self.scratch_pieces[perspective],
            );
            threat_feature_indices(
                position,
                perspective,
                king,
                &mut self.scratch_threats[perspective],
            );

            diff_features(
                &self.active_pieces[perspective],
                &self.scratch_pieces[perspective],
                &mut frame.piece_delta[perspective],
            );
            diff_features(
                &self.active_threats[perspective],
                &self.scratch_threats[perspective],
                &mut frame.threat_delta[perspective],
            );
            apply_delta(
                &mut self.accumulators[perspective],
                &frame.piece_delta[perspective],
                true,
                true,
            );
            apply_delta(
                &mut self.accumulators[perspective],
                &frame.threat_delta[perspective],
                false,
                true,
            );

            std::mem::swap(
                &mut self.active_pieces[perspective],
                &mut self.scratch_pieces[perspective],
            );
            std::mem::swap(
                &mut self.scratch_pieces[perspective],
                &mut frame.previous_pieces[perspective],
            );
            std::mem::swap(
                &mut self.active_threats[perspective],
                &mut self.scratch_threats[perspective],
            );
            std::mem::swap(
                &mut self.scratch_threats[perspective],
                &mut frame.previous_threats[perspective],
            );
        }
        self.depth += 1;
    }

    pub fn pop(&mut self) {
        if !self.ready || self.depth == 0 {
            return;
        }
        self.depth -= 1;
        let frame = &mut self.frames[self.depth];
        for perspective in 0..2 {
            apply_delta(
                &mut self.accumulators[perspective],
                &frame.piece_delta[perspective],
                true,
                false,
            );
            apply_delta(
                &mut self.accumulators[perspective],
                &frame.threat_delta[perspective],
                false,
                false,
            );
            std::mem::swap(
                &mut self.active_pieces[perspective],
                &mut frame.previous_pieces[perspective],
            );
            std::mem::swap(
                &mut self.active_threats[perspective],
                &mut frame.previous_threats[perspective],
            );
        }
    }

    pub fn evaluate(&self, position: &Position) -> Option<i32> {
        self.ready
            .then(|| output_transformer(position, &self.accumulators))
    }
}

fn diff_features(old: &[usize], new: &[usize], delta: &mut FeatureDelta) {
    delta.clear();
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old.len() && new_index < new.len() {
        match old[old_index].cmp(&new[new_index]) {
            std::cmp::Ordering::Less => {
                delta.removed.push(old[old_index]);
                old_index += 1;
            }
            std::cmp::Ordering::Greater => {
                delta.added.push(new[new_index]);
                new_index += 1;
            }
            std::cmp::Ordering::Equal => {
                old_index += 1;
                new_index += 1;
            }
        }
    }
    delta.removed.extend_from_slice(&old[old_index..]);
    delta.added.extend_from_slice(&new[new_index..]);
}

fn apply_features(accumulator: &mut [i32; FEATURES], features: &[usize], piece: bool) {
    for &feature in features {
        let row = if piece {
            PIECE_OFFSET + feature * FEATURES * 2
        } else {
            feature * FEATURES
        };
        for (i, value) in accumulator.iter_mut().enumerate() {
            *value += if piece {
                model_i16(row + i * 2)
            } else {
                model_i8(row + i)
            };
        }
    }
}

fn apply_delta(
    accumulator: &mut [i32; FEATURES],
    delta: &FeatureDelta,
    piece: bool,
    forward: bool,
) {
    if forward {
        for &feature in &delta.removed {
            apply_feature(accumulator, feature, piece, -1);
        }
        for &feature in &delta.added {
            apply_feature(accumulator, feature, piece, 1);
        }
    } else {
        for &feature in &delta.added {
            apply_feature(accumulator, feature, piece, -1);
        }
        for &feature in &delta.removed {
            apply_feature(accumulator, feature, piece, 1);
        }
    }
}

fn apply_feature(accumulator: &mut [i32; FEATURES], feature: usize, piece: bool, sign: i32) {
    let row = if piece {
        PIECE_OFFSET + feature * FEATURES * 2
    } else {
        feature * FEATURES
    };
    for (i, value) in accumulator.iter_mut().enumerate() {
        let weight = if piece {
            model_i16(row + i * 2)
        } else {
            model_i8(row + i)
        };
        *value += sign * weight;
    }
}

fn output_transformer(position: &Position, accumulators: &[[i32; FEATURES]; 2]) -> i32 {
    let stm = if position.side == Side::White { 0 } else { 1 };
    let mut transformed = [0u8; FEATURES];
    for flip in 0..2 {
        let values = &accumulators[stm ^ flip];
        let base = flip * FEATURES / 2;
        for i in 0..FEATURES / 2 {
            let left = values[i].clamp(0, 255);
            let right = values[i + FEATURES / 2].clamp(0, 255);
            transformed[base + i] = ((left * right) >> 9) as u8;
        }
    }

    let pieces = position.piece_count().min(32) as usize;
    let bucket = OUTPUT_BUCKET_LAYOUT[pieces];
    let mut pre = [0i32; L2];
    for group in 0..FEATURES / 4 {
        let start = group * 4;
        if transformed[start..start + 4].iter().all(|x| *x == 0) {
            continue;
        }
        for (out, value) in pre.iter_mut().enumerate() {
            let weight_start = L1_OFFSET + (bucket * L2 * FEATURES) + group * L2 * 4 + out * 4;
            for lane in 0..4 {
                *value += transformed[start + lane] as i32 * model_i8(weight_start + lane);
            }
        }
    }

    let mut l1 = [0f32; L2];
    for (i, value) in l1.iter_mut().enumerate() {
        *value = (pre[i] as f32)
            .mul_add(SCALE, model_f32(L1_BIAS_OFFSET + (bucket * L2 + i) * 4))
            .clamp(0.0, 1.0);
    }

    let mut l2 = [0f32; L3];
    for (j, value) in l2.iter_mut().enumerate() {
        *value = model_f32(L2_BIAS_OFFSET + (bucket * L3 + j) * 4);
    }
    for (i, &l1_value) in l1.iter().enumerate() {
        for (j, value) in l2.iter_mut().enumerate() {
            let weight = L2_WEIGHT_OFFSET + ((bucket * L2 + i) * L3 + j) * 4;
            *value = model_f32(weight).mul_add(l1_value, *value);
        }
    }
    l2.iter_mut().for_each(|v| *v = v.clamp(0.0, 1.0));

    let mut sums = [0f32; 16];
    for i in 0..L3 {
        let weight = L3_WEIGHT_OFFSET + (bucket * L3 + i) * 4;
        sums[i % 16] = model_f32(weight).mul_add(l2[i], sums[i % 16]);
    }
    let mut stride = 8;
    while stride > 0 {
        for i in 0..stride {
            sums[i] += sums[i + stride];
        }
        stride /= 2;
    }
    ((sums[0] + model_f32(L3_BIAS_OFFSET + bucket * 4)) * 380.0) as i32
}

pub fn evaluate(position: &Position) -> Option<i32> {
    let mut evaluator = NnueEvaluator::default();
    evaluator
        .reset(position)
        .then(|| evaluator.evaluate(position))
        .flatten()
}

#[cfg(all(test, valox_nnue_weights))]
mod tests {
    use super::NnueEvaluator;
    use crate::position::{parse_uci_move, Position};

    #[test]
    fn incremental_accumulator_matches_full_refresh_after_legal_moves() {
        let fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        ];
        for fen in fens {
            let position = Position::from_fen(fen).expect("valid test FEN");
            let mut incremental = NnueEvaluator::default();
            assert!(incremental.reset(&position));
            for mv in position.legal_moves() {
                let next = position.play(mv);
                incremental.push(&next);
                let mut refreshed = NnueEvaluator::default();
                assert!(refreshed.reset(&next));
                assert_eq!(
                    incremental.evaluate(&next),
                    refreshed.evaluate(&next),
                    "NNUE accumulator mismatch after {} in {fen}",
                    mv.uci()
                );
                incremental.pop();
            }
        }
    }

    #[test]
    fn incremental_accumulator_stays_correct_through_promotion_line() {
        let position = Position::from_fen("4k3/P7/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let promotion = parse_uci_move(&position, "a7a8q").expect("legal queen promotion");
        let promoted = position.play(promotion);
        let mut incremental = NnueEvaluator::default();
        assert!(incremental.reset(&position));
        incremental.push(&promoted);
        let mut refreshed = NnueEvaluator::default();
        assert!(refreshed.reset(&promoted));
        assert_eq!(
            incremental.evaluate(&promoted),
            refreshed.evaluate(&promoted)
        );
    }
}

#[cfg(all(test, not(valox_nnue_weights)))]
mod tests_without_weights {
    use crate::position::Position;

    #[test]
    fn source_only_build_uses_the_classical_evaluator() {
        let position =
            Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
                .expect("valid start position");
        assert_eq!(super::evaluate(&position), None);
        assert_eq!(position.evaluate(), position.evaluate_classical());
    }
}
