//! Scalar inference for the locally installed Reckless v60 NNUE weights.
//! The feature extraction and forward pass are implemented in Valox.

use crate::position::{Piece, Position, Side, Undo};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;

#[inline]
fn inside_board(file: i8, rank: i8) -> bool {
    (0..8).contains(&file) && (0..8).contains(&rank)
}

// Model weights are intentionally not tracked in the public source repository.
// `build.rs` can embed a local copy (default path or `VALOX_NNUE_PATH`); a
// release build can instead ship without weights and load them at runtime.
#[cfg(valox_nnue_weights)]
const EMBEDDED_MODEL: &[u8] = include!(concat!(env!("OUT_DIR"), "/nnue_model.rs"));
#[cfg(not(valox_nnue_weights))]
const EMBEDDED_MODEL: &[u8] = &[];

/// Weights loaded at runtime from a file. They take precedence over the model
/// embedded at build time, so a weight-free binary can use an external network.
struct ModelHandle {
    bytes: Box<[u8]>,
}

static EXTERNAL_MODEL: AtomicPtr<ModelHandle> = AtomicPtr::new(std::ptr::null_mut());

/// Load a same-layout NNUE file. The handle is leaked because evaluators hold a
/// `'static` view of the bytes; a reload leaks the previous handle so an
/// in-flight evaluator can finish safely.
pub fn load_model(path: &std::path::Path) -> Result<usize, String> {
    let data =
        std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if data.len() != MODEL_BYTES {
        return Err(format!(
            "{} is {} bytes, expected {MODEL_BYTES}",
            path.display(),
            data.len()
        ));
    }
    let length = data.len();
    let handle = Box::new(ModelHandle {
        bytes: data.into_boxed_slice(),
    });
    EXTERNAL_MODEL.swap(Box::into_raw(handle), Ordering::AcqRel);
    Ok(length)
}

/// The active weights: the runtime model when present, otherwise the embedded
/// one. An empty slice selects the classical evaluator.
#[inline]
pub fn model() -> &'static [u8] {
    let handle = EXTERNAL_MODEL.load(Ordering::Acquire);
    if handle.is_null() {
        return EMBEDDED_MODEL;
    }
    // SAFETY: every handle stored in EXTERNAL_MODEL is leaked and never freed.
    unsafe { &(*handle).bytes }
}

/// Whether the active model has the expected layout and can be evaluated.
pub fn weights_available() -> bool {
    model().len() == MODEL_BYTES
}
const FEATURES: usize = 768;
const THREAT_FEATURES: usize = 66_864;
const PIECE_MARK_WORDS: usize = INPUT_BUCKETS * FEATURES / 64;
const THREAT_MARK_WORDS: usize = THREAT_FEATURES.div_ceil(64);
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

fn compute_attack_map(piece: Piece, square: u8, side: usize) -> [bool; 64] {
    let mut result = [false; 64];
    let file = (square % 8) as i8;
    let rank = (square / 8) as i8;
    match piece {
        Piece::Pawn => {
            if rank > 0 && rank < 7 {
                let target_rank = rank + if side == 0 { 1 } else { -1 };
                for target_file in [file - 1, file + 1] {
                    if inside_board(target_file, target_rank) {
                        result[(target_rank * 8 + target_file) as usize] = true;
                    }
                }
            }
        }
        Piece::Knight => {
            for (df, dr) in [
                (1, 2),
                (2, 1),
                (2, -1),
                (1, -2),
                (-1, -2),
                (-2, -1),
                (-2, 1),
                (-1, 2),
            ] {
                let (target_file, target_rank) = (file + df, rank + dr);
                if inside_board(target_file, target_rank) {
                    result[(target_rank * 8 + target_file) as usize] = true;
                }
            }
        }
        Piece::King => {
            for df in -1..=1 {
                for dr in -1..=1 {
                    let (target_file, target_rank) = (file + df, rank + dr);
                    if (df != 0 || dr != 0) && inside_board(target_file, target_rank) {
                        result[(target_rank * 8 + target_file) as usize] = true;
                    }
                }
            }
        }
        Piece::Bishop | Piece::Rook | Piece::Queen => {
            let directions: &[(i8, i8)] = match piece {
                Piece::Bishop => &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
                Piece::Rook => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
                Piece::Queen => &[
                    (1, 1),
                    (1, -1),
                    (-1, 1),
                    (-1, -1),
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                ],
                _ => unreachable!(),
            };
            for &(df, dr) in directions {
                let (mut target_file, mut target_rank) = (file + df, rank + dr);
                while inside_board(target_file, target_rank) {
                    result[(target_rank * 8 + target_file) as usize] = true;
                    target_file += df;
                    target_rank += dr;
                }
            }
        }
    }
    result
}

/// Geometric attack map for one attacker, cached by piece kind, square and side.
/// Threat generation asks for the same maps at every node, and the maps never
/// depend on the rest of the board, so build the 12 x 64 maps once.
fn attack_map(kind: usize, square: u8, side: usize) -> &'static [bool; 64] {
    static MAPS: OnceLock<[[[bool; 64]; 64]; 12]> = OnceLock::new();
    let maps = MAPS.get_or_init(|| {
        let mut maps = [[[false; 64]; 64]; 12];
        for side in 0..2 {
            for kind in 0..6 {
                for (square, map) in maps[side * 6 + kind].iter_mut().enumerate() {
                    *map = compute_attack_map(PIECE_TYPES[kind], square as u8, side);
                }
            }
        }
        maps
    });
    &maps[side * 6 + kind][square as usize]
}

/// Rank of a target inside its attack map (the number of attacked squares before
/// it). This used to be recomputed by scanning up to 64 booleans for every
/// threat candidate; the rank only depends on the map, so cache it too.
fn attack_rank(kind: usize, square: u8, side: usize, target: usize) -> Option<usize> {
    static RANKS: OnceLock<[[[u8; 64]; 64]; 12]> = OnceLock::new();
    let ranks = RANKS.get_or_init(|| {
        let mut ranks = [[[u8::MAX; 64]; 64]; 12];
        for side in 0..2 {
            for kind in 0..6 {
                for square in 0..64u8 {
                    let map = compute_attack_map(PIECE_TYPES[kind], square, side);
                    let mut count = 0u8;
                    for target in 0..64 {
                        if map[target] {
                            ranks[side * 6 + kind][square as usize][target] = count;
                            count += 1;
                        }
                    }
                }
            }
        }
        ranks
    });
    let rank = ranks[side * 6 + kind][square as usize][target];
    (rank != u8::MAX).then_some(rank as usize)
}

#[cfg(test)]
fn square_attacks(piece: Piece, square: u8, side: usize) -> [bool; 64] {
    let kind = PIECE_TYPES
        .iter()
        .position(|candidate| *candidate == piece)
        .expect("known piece kind");
    *attack_map(kind, square, side)
}

#[cfg(test)]
fn square_attacks_reference(piece: Piece, square: u8, side: usize) -> [bool; 64] {
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
            let mut count = 0;
            if kind < PIECE_TYPES.len() {
                for sq in 0..64u8 {
                    layout.per_square[code][sq as usize] = count;
                    count += attack_map(kind, sq, code_side(code))
                        .iter()
                        .filter(|x| **x)
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

#[cfg(test)]
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
    let attacker_type = code_type(attacker).checked_sub(1)?;
    if attacker_type >= 6 {
        return None;
    }
    threat_feature_index(attacker, from, to, attacked ^ (8 * perspective))
}

#[inline]
fn threat_feature_index(attacker: usize, from: usize, to: usize, attacked: usize) -> Option<usize> {
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
    let square_rank = attack_rank(attacker_type, from as u8, code_side(attacker), to)?;
    let target_side = code_side(attacked);
    let within_type =
        (target_side * (TARGET_COUNTS[attacker_type] / 2) + map as usize) * layout.span[attacker];
    let index =
        layout.base[attacker] + within_type + layout.per_square[attacker][from] + square_rank;
    (index < THREAT_FEATURES).then_some(index)
}

fn model_i8(model: &[u8], offset: usize) -> i32 {
    model[offset] as i8 as i32
}

fn model_i16(model: &[u8], offset: usize) -> i32 {
    i16::from_le_bytes([model[offset], model[offset + 1]]) as i32
}

fn model_f32(model: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        model[offset],
        model[offset + 1],
        model[offset + 2],
        model[offset + 3],
    ])
}

fn piece_kind(piece: Piece) -> usize {
    match piece {
        Piece::Pawn => 0,
        Piece::Knight => 1,
        Piece::Bishop => 2,
        Piece::Rook => 3,
        Piece::Queen => 4,
        Piece::King => 5,
    }
}

fn piece_feature_index(
    piece: Piece,
    side: Side,
    square: u8,
    perspective: usize,
    king: u8,
) -> usize {
    let flip = king_orientation(king) ^ (56 * perspective as u8);
    let input_bucket = INPUT_BUCKET_LAYOUT[(king ^ flip) as usize];
    let own = (side == Side::White) == (perspective == 0);
    input_bucket * FEATURES
        + usize::from(!own) * 384
        + piece_kind(piece) * 64
        + (square ^ flip) as usize
}

fn piece_feature_indices(
    position: &Position,
    perspective: usize,
    king: u8,
    features: &mut Vec<usize>,
) {
    features.clear();
    for square in 0..64u8 {
        if let Some((piece, side)) = position.piece_at(square) {
            features.push(piece_feature_index(piece, side, square, perspective, king));
        }
    }
}

fn threat_feature_indices(
    position: &Position,
    perspective: usize,
    king: u8,
    features: &mut Vec<usize>,
) {
    features.clear();
    for from in 0..64u8 {
        threat_features_for_square(position, perspective, king, from, features);
    }
}

/// Append every threat feature whose attacker stands on `from`.
fn threat_features_for_square(
    position: &Position,
    perspective: usize,
    king: u8,
    from: u8,
    features: &mut Vec<usize>,
) {
    let Some((attacker_piece, attacker_side)) = position.piece_at(from) else {
        return;
    };
    let attacker_code = piece_code(attacker_piece, attacker_side);
    let file = (from % 8) as i8;
    let rank = (from / 8) as i8;
    match attacker_piece {
        Piece::Pawn => {
            let dr = if attacker_side == Side::White { 1 } else { -1 };
            for df in [-1, 1] {
                let (target_file, target_rank) = (file + df, rank + dr);
                if inside_board(target_file, target_rank) {
                    push_threat_feature(
                        position,
                        attacker_code,
                        from,
                        (target_rank * 8 + target_file) as u8,
                        perspective,
                        king,
                        features,
                    );
                }
            }
        }
        Piece::Knight => {
            for (df, dr) in [
                (1, 2),
                (2, 1),
                (2, -1),
                (1, -2),
                (-1, -2),
                (-2, -1),
                (-2, 1),
                (-1, 2),
            ] {
                let (target_file, target_rank) = (file + df, rank + dr);
                if inside_board(target_file, target_rank) {
                    push_threat_feature(
                        position,
                        attacker_code,
                        from,
                        (target_rank * 8 + target_file) as u8,
                        perspective,
                        king,
                        features,
                    );
                }
            }
        }
        Piece::King => {
            for df in -1..=1 {
                for dr in -1..=1 {
                    if df == 0 && dr == 0 {
                        continue;
                    }
                    let (target_file, target_rank) = (file + df, rank + dr);
                    if inside_board(target_file, target_rank) {
                        push_threat_feature(
                            position,
                            attacker_code,
                            from,
                            (target_rank * 8 + target_file) as u8,
                            perspective,
                            king,
                            features,
                        );
                    }
                }
            }
        }
        Piece::Bishop | Piece::Rook | Piece::Queen => {
            let directions: &[(i8, i8)] = match attacker_piece {
                Piece::Bishop => &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
                Piece::Rook => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
                Piece::Queen => &[
                    (1, 1),
                    (1, -1),
                    (-1, 1),
                    (-1, -1),
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                ],
                _ => unreachable!(),
            };
            for &(df, dr) in directions {
                let (mut target_file, mut target_rank) = (file + df, rank + dr);
                while inside_board(target_file, target_rank) {
                    let to = (target_rank * 8 + target_file) as u8;
                    if position.piece_at(to).is_some() {
                        push_threat_feature(
                            position,
                            attacker_code,
                            from,
                            to,
                            perspective,
                            king,
                            features,
                        );
                        break;
                    }
                    target_file += df;
                    target_rank += dr;
                }
            }
        }
    }
}

fn push_threat_feature(
    position: &Position,
    attacker_code: usize,
    from: u8,
    to: u8,
    perspective: usize,
    king: u8,
    features: &mut Vec<usize>,
) {
    let Some((attacked_piece, attacked_side)) = position.piece_at(to) else {
        return;
    };
    if attacked_piece == Piece::King {
        return;
    }
    let orient = king_orientation(king) ^ (56 * perspective as u8);
    let attacker = attacker_code ^ (8 * perspective);
    if let Some(feature) = threat_feature_index(
        attacker,
        (from ^ orient) as usize,
        (to ^ orient) as usize,
        piece_code(attacked_piece, attacked_side) ^ (8 * perspective),
    ) {
        features.push(feature);
    }
}

/// Steps for the eight slider directions, indexed by `ray_direction`.
const RAY_STEPS: [(i8, i8); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

fn ray_direction(from: u8, target: u8) -> usize {
    let df = (target % 8) as i8 - (from % 8) as i8;
    let dr = (target / 8) as i8 - (from / 8) as i8;
    match (df.signum(), dr.signum()) {
        (-1, -1) => 0,
        (0, -1) => 1,
        (1, -1) => 2,
        (-1, 0) => 3,
        (1, 0) => 4,
        (-1, 1) => 5,
        (0, 1) => 6,
        (1, 1) => 7,
        _ => unreachable!("attacker and target are distinct squares"),
    }
}

/// Append the slider threat edge for each selected direction: the first
/// occupied square along that ray, if any.
fn push_slider_ray_features(
    position: &Position,
    perspective: usize,
    king: u8,
    from: u8,
    rays: u8,
    features: &mut Vec<usize>,
) {
    let Some((piece, side)) = position.piece_at(from) else {
        return;
    };
    let attacker_code = piece_code(piece, side);
    let file = (from % 8) as i8;
    let rank = (from / 8) as i8;
    for (index, (df, dr)) in RAY_STEPS.iter().copied().enumerate() {
        if rays & (1 << index) == 0 {
            continue;
        }
        let (mut target_file, mut target_rank) = (file + df, rank + dr);
        while inside_board(target_file, target_rank) {
            let to = (target_rank * 8 + target_file) as u8;
            if position.piece_at(to).is_some() {
                push_threat_feature(
                    position,
                    attacker_code,
                    from,
                    to,
                    perspective,
                    king,
                    features,
                );
                break;
            }
            target_file += df;
            target_rank += dr;
        }
    }
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
}

impl AccumulatorFrame {
    fn new() -> Self {
        Self {
            piece_delta: std::array::from_fn(|_| FeatureDelta::new(32)),
            threat_delta: std::array::from_fn(|_| FeatureDelta::new(512)),
        }
    }
}

/// Search-local NNUE state. Each push applies only feature additions/removals;
/// pop reverses that delta when the search backs up the tree.
pub struct NnueEvaluator {
    accumulators: [[i16; FEATURES]; 2],
    piece_marks: [u64; PIECE_MARK_WORDS],
    threat_marks: [u64; THREAT_MARK_WORDS],
    mark_touched: Vec<u32>,
    old_features: Vec<usize>,
    new_features: Vec<usize>,
    frames: Vec<AccumulatorFrame>,
    depth: usize,
    ready: bool,
}

impl Default for NnueEvaluator {
    fn default() -> Self {
        Self {
            accumulators: [[0; FEATURES]; 2],
            piece_marks: [0; PIECE_MARK_WORDS],
            threat_marks: [0; THREAT_MARK_WORDS],
            mark_touched: Vec::with_capacity(256),
            old_features: Vec::with_capacity(256),
            new_features: Vec::with_capacity(256),
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
        let model = model();
        if model.len() != MODEL_BYTES {
            return false;
        }
        self.piece_marks = [0; PIECE_MARK_WORDS];
        self.threat_marks = [0; THREAT_MARK_WORDS];
        self.mark_touched.clear();
        for (perspective, side) in [Side::White, Side::Black].into_iter().enumerate() {
            let Some(king) = position.king_square(side) else {
                return false;
            };
            // The full refresh still sums in i32 so a transient partial sum
            // cannot wrap. Reckless's i16 accumulator is the reference for the
            // final range; the incremental updates below rely on that bound.
            let mut accumulator = [0i32; FEATURES];
            for (i, value) in accumulator.iter_mut().enumerate() {
                *value = model_i16(model, BIAS_OFFSET + i * 2);
            }
            piece_feature_indices(position, perspective, king, &mut self.old_features);
            apply_features(model, &mut accumulator, &self.old_features, true);
            threat_feature_indices(position, perspective, king, &mut self.old_features);
            apply_features(model, &mut accumulator, &self.old_features, false);
            for (slot, value) in self.accumulators[perspective].iter_mut().zip(accumulator) {
                debug_assert!(i16::try_from(value).is_ok(), "NNUE accumulator overflow");
                *slot = value as i16;
            }
        }
        self.ready = true;
        true
    }

    pub fn push(&mut self, position: &Position, undo: Undo) {
        if !self.ready {
            return;
        }
        let model = model();
        if self.frames.len() == self.depth {
            self.frames.push(AccumulatorFrame::new());
        }
        let frame_index = self.depth;
        let mut parent = position.clone();
        parent.unmake_move(undo);

        // Squares whose contents differ between the parent and the child.
        let mut changed = [false; 64];
        let mut changed_squares = [0u8; 8];
        let mut changed_len = 0usize;
        mark_changed(
            undo.mv.from,
            &mut changed,
            &mut changed_squares,
            &mut changed_len,
        );
        mark_changed(
            undo.mv.to,
            &mut changed,
            &mut changed_squares,
            &mut changed_len,
        );
        if let Some((square, _)) = undo.captured {
            mark_changed(square, &mut changed, &mut changed_squares, &mut changed_len);
        }
        if let Some((from, to)) = undo.rook {
            mark_changed(from, &mut changed, &mut changed_squares, &mut changed_len);
            mark_changed(to, &mut changed, &mut changed_squares, &mut changed_len);
        }
        let changed_squares = &changed_squares[..changed_len];

        let frame = &mut self.frames[frame_index];
        for perspective in 0..2 {
            frame.piece_delta[perspective].clear();
            frame.threat_delta[perspective].clear();
            let side = if perspective == 0 {
                Side::White
            } else {
                Side::Black
            };
            let Some(king) = position.king_square(side) else {
                self.ready = false;
                return;
            };

            // A king move changes the input bucket and the orientation for all
            // features of its side, so that perspective is rebuilt in full.
            let own_king_moved = undo.side == side && matches!(undo.moved, Some((Piece::King, _)));
            self.old_features.clear();
            self.new_features.clear();
            if own_king_moved {
                // The king moved, so its parent square is the move origin.
                let parent_king = undo.mv.from;
                piece_feature_indices(&parent, perspective, parent_king, &mut self.old_features);
                piece_feature_indices(position, perspective, king, &mut self.new_features);
            } else {
                // Piece features change only on squares whose contents changed.
                for &square in changed_squares {
                    if let Some((piece, piece_side)) = parent.piece_at(square) {
                        self.old_features.push(piece_feature_index(
                            piece,
                            piece_side,
                            square,
                            perspective,
                            king,
                        ));
                    }
                    if let Some((piece, piece_side)) = position.piece_at(square) {
                        self.new_features.push(piece_feature_index(
                            piece,
                            piece_side,
                            square,
                            perspective,
                            king,
                        ));
                    }
                }
            }
            diff_feature_bits(
                &self.old_features,
                &self.new_features,
                &mut self.piece_marks,
                &mut self.mark_touched,
                &mut frame.piece_delta[perspective],
            );
            apply_delta(
                model,
                &mut self.accumulators[perspective],
                &frame.piece_delta[perspective],
                true,
                true,
            );

            self.old_features.clear();
            self.new_features.clear();
            if own_king_moved {
                let parent_king = undo.mv.from;
                threat_feature_indices(&parent, perspective, parent_king, &mut self.old_features);
                threat_feature_indices(position, perspective, king, &mut self.new_features);
            } else {
                // A threat feature depends on the attacker's square and type,
                // the target's square and type, and (for sliders) the first
                // occupied square on the ray. Only changed attacker squares,
                // changed target squares and rays through changed squares can
                // alter a feature. Leapers have no blocking, so for them a
                // single edge to the changed target suffices; sliders only need
                // the rays that contain a changed square. The earlier version
                // rebuilt every outgoing edge of every geometric attacker,
                // which repeated most of that work.
                for &square in changed_squares {
                    if parent.piece_at(square).is_some() {
                        threat_features_for_square(
                            &parent,
                            perspective,
                            king,
                            square,
                            &mut self.old_features,
                        );
                    }
                    if position.piece_at(square).is_some() {
                        threat_features_for_square(
                            position,
                            perspective,
                            king,
                            square,
                            &mut self.new_features,
                        );
                    }
                }
                let mut slider_rays = [0u8; 64];
                for &target in changed_squares {
                    let mut attackers = position.attackers_to_mask(target);
                    while attackers != 0 {
                        let from = attackers.trailing_zeros() as u8;
                        attackers &= attackers - 1;
                        if changed[from as usize] {
                            // Covered by the full outgoing rebuild above.
                            continue;
                        }
                        let Some((piece, side)) = position.piece_at(from) else {
                            continue;
                        };
                        match piece {
                            Piece::Pawn | Piece::Knight | Piece::King => {
                                if let Some((piece, side)) = parent.piece_at(from) {
                                    push_threat_feature(
                                        &parent,
                                        piece_code(piece, side),
                                        from,
                                        target,
                                        perspective,
                                        king,
                                        &mut self.old_features,
                                    );
                                }
                                push_threat_feature(
                                    position,
                                    piece_code(piece, side),
                                    from,
                                    target,
                                    perspective,
                                    king,
                                    &mut self.new_features,
                                );
                            }
                            Piece::Bishop | Piece::Rook | Piece::Queen => {
                                slider_rays[from as usize] |= 1 << ray_direction(from, target);
                            }
                        }
                    }
                }
                for from in 0..64u8 {
                    let rays = slider_rays[from as usize];
                    if rays == 0 {
                        continue;
                    }
                    push_slider_ray_features(
                        &parent,
                        perspective,
                        king,
                        from,
                        rays,
                        &mut self.old_features,
                    );
                    push_slider_ray_features(
                        position,
                        perspective,
                        king,
                        from,
                        rays,
                        &mut self.new_features,
                    );
                }
            }
            diff_feature_bits(
                &self.old_features,
                &self.new_features,
                &mut self.threat_marks,
                &mut self.mark_touched,
                &mut frame.threat_delta[perspective],
            );
            apply_delta(
                model,
                &mut self.accumulators[perspective],
                &frame.threat_delta[perspective],
                false,
                true,
            );
        }
        self.depth += 1;
    }

    pub fn pop(&mut self) {
        if !self.ready || self.depth == 0 {
            return;
        }
        let model = model();
        self.depth -= 1;
        let frame = &mut self.frames[self.depth];
        for perspective in 0..2 {
            apply_delta(
                model,
                &mut self.accumulators[perspective],
                &frame.piece_delta[perspective],
                true,
                false,
            );
            apply_delta(
                model,
                &mut self.accumulators[perspective],
                &frame.threat_delta[perspective],
                false,
                false,
            );
        }
    }

    pub fn evaluate(&self, position: &Position) -> Option<i32> {
        self.ready
            .then(|| output_transformer(position, &self.accumulators))
    }
}

/// Set-difference of two feature lists using a bitset instead of sorting both
/// lists. Feature indices are unique within a position, so one bit per index is
/// sufficient. Matched bits are cleared immediately; the bits still set after
/// scanning the new list are exactly the removed features.
fn diff_feature_bits(
    old: &[usize],
    new: &[usize],
    marks: &mut [u64],
    touched: &mut Vec<u32>,
    delta: &mut FeatureDelta,
) {
    delta.clear();
    touched.clear();
    for &feature in old {
        let word = feature / 64;
        let bit = 1u64 << (feature % 64);
        if marks[word] == 0 {
            touched.push(word as u32);
        }
        marks[word] |= bit;
    }
    for &feature in new {
        let word = feature / 64;
        let bit = 1u64 << (feature % 64);
        if marks[word] & bit != 0 {
            marks[word] &= !bit;
        } else {
            delta.added.push(feature);
        }
    }
    for &word in touched.iter() {
        let mut bits = marks[word as usize];
        let base = word as usize * 64;
        while bits != 0 {
            let bit = bits.trailing_zeros() as usize;
            delta.removed.push(base + bit);
            bits &= bits - 1;
        }
        marks[word as usize] = 0;
    }
}

fn mark_changed(square: u8, changed: &mut [bool; 64], list: &mut [u8; 8], len: &mut usize) {
    if !changed[square as usize] {
        changed[square as usize] = true;
        list[*len] = square;
        *len += 1;
    }
}

fn apply_features(
    model: &[u8],
    accumulator: &mut [i32; FEATURES],
    features: &[usize],
    piece: bool,
) {
    for &feature in features {
        let row = if piece {
            PIECE_OFFSET + feature * FEATURES * 2
        } else {
            feature * FEATURES
        };
        for (i, value) in accumulator.iter_mut().enumerate() {
            *value += if piece {
                model_i16(model, row + i * 2)
            } else {
                model_i8(model, row + i)
            };
        }
    }
}

fn apply_delta(
    model: &[u8],
    accumulator: &mut [i16; FEATURES],
    delta: &FeatureDelta,
    piece: bool,
    forward: bool,
) {
    if forward {
        for &feature in &delta.removed {
            apply_feature(model, accumulator, feature, piece, -1);
        }
        for &feature in &delta.added {
            apply_feature(model, accumulator, feature, piece, 1);
        }
    } else {
        for &feature in &delta.added {
            apply_feature(model, accumulator, feature, piece, -1);
        }
        for &feature in &delta.removed {
            apply_feature(model, accumulator, feature, piece, 1);
        }
    }
}

fn apply_feature(
    model: &[u8],
    accumulator: &mut [i16; FEATURES],
    feature: usize,
    piece: bool,
    sign: i32,
) {
    #[cfg(target_arch = "x86_64")]
    if avx2_feature_updates() {
        // SAFETY: `avx2_feature_updates` checked AVX2 support at runtime.
        unsafe { apply_feature_avx2(model, accumulator, feature, piece, sign) };
        return;
    }
    apply_feature_scalar(model, accumulator, feature, piece, sign);
}

#[cfg(target_arch = "x86_64")]
fn avx2_feature_updates() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("VALOX_NNUE_SCALAR_UPDATES").is_none()
            && std::arch::is_x86_feature_detected!("avx2")
    })
}

fn apply_feature_scalar(
    model: &[u8],
    accumulator: &mut [i16; FEATURES],
    feature: usize,
    piece: bool,
    sign: i32,
) {
    let row = if piece {
        PIECE_OFFSET + feature * FEATURES * 2
    } else {
        feature * FEATURES
    };
    if piece {
        let weights = &model[row..row + FEATURES * 2];
        for (i, value) in accumulator.iter_mut().enumerate() {
            let offset = i * 2;
            let weight = i16::from_le_bytes([weights[offset], weights[offset + 1]]);
            *value = if sign >= 0 {
                value.wrapping_add(weight)
            } else {
                value.wrapping_sub(weight)
            };
        }
    } else {
        let weights = &model[row..row + FEATURES];
        for (value, &weight) in accumulator.iter_mut().zip(weights) {
            let weight = i16::from(weight as i8);
            *value = if sign >= 0 {
                value.wrapping_add(weight)
            } else {
                value.wrapping_sub(weight)
            };
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn apply_feature_avx2(
    model: &[u8],
    accumulator: &mut [i16; FEATURES],
    feature: usize,
    piece: bool,
    sign: i32,
) {
    use std::arch::x86_64::{
        __m128i, __m256i, _mm256_add_epi16, _mm256_cvtepi8_epi16, _mm256_loadu_si256,
        _mm256_storeu_si256, _mm256_sub_epi16, _mm_loadu_si128,
    };

    let row = if piece {
        PIECE_OFFSET + feature * FEATURES * 2
    } else {
        feature * FEATURES
    };
    for i in (0..FEATURES).step_by(16) {
        let values = if piece {
            _mm256_loadu_si256(model.as_ptr().add(row + i * 2) as *const __m256i)
        } else {
            _mm256_cvtepi8_epi16(_mm_loadu_si128(
                model.as_ptr().add(row + i) as *const __m128i
            ))
        };
        let slot = accumulator.as_mut_ptr().add(i) as *mut __m256i;
        let current = _mm256_loadu_si256(slot);
        let updated = if sign >= 0 {
            _mm256_add_epi16(current, values)
        } else {
            _mm256_sub_epi16(current, values)
        };
        _mm256_storeu_si256(slot, updated);
    }
}

fn l2_forward_scalar(model: &[u8], bucket: usize, input: &[f32; L2]) -> [f32; L3] {
    let mut output = [0.0f32; L3];
    for (j, value) in output.iter_mut().enumerate() {
        *value = model_f32(model, L2_BIAS_OFFSET + (bucket * L3 + j) * 4);
    }
    for (i, &input_value) in input.iter().enumerate() {
        for (j, value) in output.iter_mut().enumerate() {
            let weight = L2_WEIGHT_OFFSET + ((bucket * L2 + i) * L3 + j) * 4;
            *value = model_f32(model, weight).mul_add(input_value, *value);
        }
    }
    output
        .iter_mut()
        .for_each(|value| *value = value.clamp(0.0, 1.0));
    output
}

#[cfg(target_arch = "x86_64")]
#[cfg(any(not(feature = "nnue-scalar"), test))]
#[target_feature(enable = "avx2,fma")]
unsafe fn l2_forward_avx2(model: &[u8], bucket: usize, input: &[f32; L2]) -> [f32; L3] {
    use std::arch::x86_64::{
        _mm256_fmadd_ps, _mm256_loadu_ps, _mm256_max_ps, _mm256_min_ps, _mm256_set1_ps,
        _mm256_storeu_ps,
    };

    let bias = model
        .as_ptr()
        .add(L2_BIAS_OFFSET + bucket * L3 * 4)
        .cast::<f32>();
    let mut values = [
        _mm256_loadu_ps(bias),
        _mm256_loadu_ps(bias.add(8)),
        _mm256_loadu_ps(bias.add(16)),
        _mm256_loadu_ps(bias.add(24)),
    ];
    for (i, &input_value) in input.iter().enumerate() {
        let row = model
            .as_ptr()
            .add(L2_WEIGHT_OFFSET + (bucket * L2 + i) * L3 * 4)
            .cast::<f32>();
        let multiplier = _mm256_set1_ps(input_value);
        for (chunk, value) in values.iter_mut().enumerate() {
            *value = _mm256_fmadd_ps(_mm256_loadu_ps(row.add(chunk * 8)), multiplier, *value);
        }
    }
    let mut output = [0.0f32; L3];
    for (chunk, value) in values.iter().enumerate() {
        let clipped = _mm256_min_ps(
            _mm256_max_ps(*value, _mm256_set1_ps(0.0)),
            _mm256_set1_ps(1.0),
        );
        _mm256_storeu_ps(output.as_mut_ptr().add(chunk * 8), clipped);
    }
    output
}

fn l2_forward(model: &[u8], bucket: usize, input: &[f32; L2]) -> [f32; L3] {
    #[cfg(all(target_arch = "x86_64", not(feature = "nnue-scalar")))]
    if std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma") {
        // SAFETY: both required target features were checked at runtime above.
        return unsafe { l2_forward_avx2(model, bucket, input) };
    }
    l2_forward_scalar(model, bucket, input)
}

fn l1_forward_scalar(model: &[u8], bucket: usize, transformed: &[u8; FEATURES]) -> [i32; L2] {
    let mut pre = [0i32; L2];
    for group in 0..FEATURES / 4 {
        let start = group * 4;
        if transformed[start..start + 4].iter().all(|x| *x == 0) {
            continue;
        }
        for (out, value) in pre.iter_mut().enumerate() {
            let weight_start = L1_OFFSET + (bucket * L2 * FEATURES) + group * L2 * 4 + out * 4;
            for lane in 0..4 {
                *value += transformed[start + lane] as i32 * model_i8(model, weight_start + lane);
            }
        }
    }
    pre
}

#[cfg(target_arch = "x86_64")]
#[cfg(any(not(feature = "nnue-scalar"), test))]
#[target_feature(enable = "avx2")]
unsafe fn l1_forward_avx2(model: &[u8], bucket: usize, transformed: &[u8; FEATURES]) -> [i32; L2] {
    use std::arch::x86_64::{
        __m256i, _mm256_add_epi32, _mm256_loadu_si256, _mm256_madd_epi16, _mm256_maddubs_epi16,
        _mm256_set1_epi16, _mm256_set1_epi32, _mm256_setzero_si256, _mm256_storeu_si256,
    };

    // Transformed inputs are at most (255 * 255) >> 9 = 127, so a pair of
    // products cannot saturate the signed 16-bit intermediate. This makes the
    // vector dot product bit-identical to the scalar loop.
    let ones = _mm256_set1_epi16(1);
    let mut low = _mm256_setzero_si256();
    let mut high = _mm256_setzero_si256();
    let base = L1_OFFSET + bucket * L2 * FEATURES;
    for group in 0..FEATURES / 4 {
        let start = group * 4;
        let lanes = u32::from_le_bytes([
            transformed[start],
            transformed[start + 1],
            transformed[start + 2],
            transformed[start + 3],
        ]);
        if lanes == 0 {
            continue;
        }
        let pattern = _mm256_set1_epi32(lanes as i32);
        let weights = model.as_ptr().add(base + group * L2 * 4) as *const __m256i;
        let low_products = _mm256_maddubs_epi16(pattern, _mm256_loadu_si256(weights));
        low = _mm256_add_epi32(low, _mm256_madd_epi16(low_products, ones));
        let high_products = _mm256_maddubs_epi16(pattern, _mm256_loadu_si256(weights.add(1)));
        high = _mm256_add_epi32(high, _mm256_madd_epi16(high_products, ones));
    }
    let mut pre = [0i32; L2];
    _mm256_storeu_si256(pre.as_mut_ptr() as *mut __m256i, low);
    _mm256_storeu_si256(pre.as_mut_ptr().add(8) as *mut __m256i, high);
    pre
}

fn l1_forward(model: &[u8], bucket: usize, transformed: &[u8; FEATURES]) -> [i32; L2] {
    #[cfg(all(target_arch = "x86_64", not(feature = "nnue-scalar")))]
    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 support was checked above.
        return unsafe { l1_forward_avx2(model, bucket, transformed) };
    }
    l1_forward_scalar(model, bucket, transformed)
}

fn transform_accumulators_scalar(
    stm: usize,
    accumulators: &[[i16; FEATURES]; 2],
) -> [u8; FEATURES] {
    let mut transformed = [0u8; FEATURES];
    for flip in 0..2 {
        let values = &accumulators[stm ^ flip];
        let base = flip * FEATURES / 2;
        for i in 0..FEATURES / 2 {
            let left = i32::from(values[i].clamp(0, 255));
            let right = i32::from(values[i + FEATURES / 2].clamp(0, 255));
            transformed[base + i] = ((left * right) >> 9) as u8;
        }
    }
    transformed
}

#[cfg(target_arch = "x86_64")]
#[cfg(any(not(feature = "nnue-scalar"), test))]
#[target_feature(enable = "avx2")]
unsafe fn transform_accumulators_avx2(
    stm: usize,
    accumulators: &[[i16; FEATURES]; 2],
) -> [u8; FEATURES] {
    use std::arch::x86_64::{
        __m256i, _mm256_castsi256_si128, _mm256_extracti128_si256, _mm256_loadu_si256,
        _mm256_max_epi16, _mm256_min_epi16, _mm256_mullo_epi16, _mm256_packus_epi16,
        _mm256_set1_epi16, _mm256_srli_epi16, _mm_cvtsi128_si64,
    };

    let zero = _mm256_set1_epi16(0);
    let maximum = _mm256_set1_epi16(255);
    let mut transformed = [0u8; FEATURES];
    for flip in 0..2 {
        let values = &accumulators[stm ^ flip];
        let base = flip * FEATURES / 2;
        for i in (0..FEATURES / 2).step_by(16) {
            let left = _mm256_loadu_si256(values.as_ptr().add(i) as *const __m256i);
            let right = _mm256_loadu_si256(values.as_ptr().add(i + FEATURES / 2) as *const __m256i);
            let left = _mm256_min_epi16(_mm256_max_epi16(left, zero), maximum);
            let right = _mm256_min_epi16(_mm256_max_epi16(right, zero), maximum);
            let product = _mm256_mullo_epi16(left, right);
            let shifted = _mm256_srli_epi16(product, 9);
            // `packus` duplicates each 128-bit lane; taking the low eight
            // bytes of both lanes yields the 16 consecutive results.
            let packed = _mm256_packus_epi16(shifted, shifted);
            let low = _mm_cvtsi128_si64(_mm256_castsi256_si128(packed)) as u64;
            let high = _mm_cvtsi128_si64(_mm256_extracti128_si256(packed, 1)) as u64;
            let bytes = (u128::from(low) | (u128::from(high) << 64)).to_le_bytes();
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                transformed.as_mut_ptr().add(base + i),
                16,
            );
        }
    }
    transformed
}

fn transform_accumulators(stm: usize, accumulators: &[[i16; FEATURES]; 2]) -> [u8; FEATURES] {
    #[cfg(all(target_arch = "x86_64", not(feature = "nnue-scalar")))]
    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 support was checked above.
        return unsafe { transform_accumulators_avx2(stm, accumulators) };
    }
    transform_accumulators_scalar(stm, accumulators)
}

fn output_transformer(position: &Position, accumulators: &[[i16; FEATURES]; 2]) -> i32 {
    let model = model();
    let stm = if position.side == Side::White { 0 } else { 1 };
    let transformed = transform_accumulators(stm, accumulators);

    let pieces = position.piece_count().min(32) as usize;
    let bucket = OUTPUT_BUCKET_LAYOUT[pieces];
    let pre = l1_forward(model, bucket, &transformed);

    let mut l1 = [0f32; L2];
    for (i, value) in l1.iter_mut().enumerate() {
        *value = (pre[i] as f32)
            .mul_add(
                SCALE,
                model_f32(model, L1_BIAS_OFFSET + (bucket * L2 + i) * 4),
            )
            .clamp(0.0, 1.0);
    }

    let l2 = l2_forward(model, bucket, &l1);

    let mut sums = [0f32; 16];
    for i in 0..L3 {
        let weight = L3_WEIGHT_OFFSET + (bucket * L3 + i) * 4;
        sums[i % 16] = model_f32(model, weight).mul_add(l2[i], sums[i % 16]);
    }
    let mut stride = 8;
    while stride > 0 {
        for i in 0..stride {
            sums[i] += sums[i + stride];
        }
        stride /= 2;
    }
    ((sums[0] + model_f32(model, L3_BIAS_OFFSET + bucket * 4)) * 380.0) as i32
}

pub fn evaluate(position: &Position) -> Option<i32> {
    let mut evaluator = NnueEvaluator::default();
    evaluator
        .reset(position)
        .then(|| evaluator.evaluate(position))
        .flatten()
}

#[cfg(test)]
mod threat_generation_tests {
    use super::{
        piece_code, square_attacks, square_attacks_reference, threat_feature_indices, threat_index,
        PIECE_TYPES, TARGET_MAP,
    };
    use crate::position::{Piece, Position, Side};

    fn brute_force_threat_features(
        position: &Position,
        perspective: usize,
        king: u8,
    ) -> Vec<usize> {
        let mut features = Vec::new();
        for from in 0..64u8 {
            let Some((attacker_piece, attacker_side)) = position.piece_at(from) else {
                continue;
            };
            let attacker_kind = PIECE_TYPES
                .iter()
                .position(|piece| *piece == attacker_piece)
                .unwrap();
            for to in 0..64u8 {
                let Some((attacked_piece, attacked_side)) = position.piece_at(to) else {
                    continue;
                };
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
                if let Some(feature) = threat_index(
                    piece_code(attacker_piece, attacker_side),
                    from,
                    to,
                    piece_code(attacked_piece, attacked_side),
                    perspective,
                    king,
                ) {
                    features.push(feature);
                }
            }
        }
        features.sort_unstable();
        features
    }

    #[test]
    fn optimized_threat_generation_matches_brute_force_in_play_positions() {
        let fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/ppp2ppp/2n5/3qp3/3P4/2N2N2/PPP2PPP/R2QK2R w KQkq - 0 10",
            "8/2p2pk1/1p3np1/3Pp3/2P1P3/1P3NP1/5PK1/8 w - - 0 40",
        ];
        for fen in fens {
            let mut position = Position::from_fen(fen).expect("valid test position");
            for _ in 0..32 {
                for (perspective, side) in [Side::White, Side::Black].into_iter().enumerate() {
                    let king = (0..64u8)
                        .find(|&sq| position.piece_at(sq) == Some((Piece::King, side)))
                        .expect("king exists");
                    let mut optimized = Vec::new();
                    threat_feature_indices(&position, perspective, king, &mut optimized);
                    optimized.sort_unstable();
                    let brute = brute_force_threat_features(&position, perspective, king);
                    let missing = brute
                        .iter()
                        .filter(|feature| !optimized.contains(feature))
                        .copied()
                        .collect::<Vec<_>>();
                    assert_eq!(
                        optimized,
                        brute,
                        "threat features differ for perspective {perspective} in {fen}; missing {missing:?}"
                    );
                }
                let moves = position.legal_moves();
                if moves.is_empty() {
                    break;
                }
                let index = (position.hash() as usize) % moves.len();
                position = position.play(moves[index]);
            }
        }
    }

    #[test]
    fn direct_attack_maps_match_full_square_scan() {
        for piece in PIECE_TYPES {
            for square in 0..64u8 {
                for side in 0..2 {
                    assert_eq!(
                        square_attacks(piece, square, side),
                        square_attacks_reference(piece, square, side),
                        "attack map differs for {piece:?} on square {square}, side {side}"
                    );
                }
            }
        }
    }

    #[test]
    fn cached_attack_ranks_match_prefix_scan() {
        for side in 0..2 {
            for kind in 0..6 {
                for square in 0..64u8 {
                    let map = super::attack_map(kind, square, side);
                    for target in 0..64usize {
                        let expected = map[target]
                            .then(|| map[..target].iter().filter(|attacked| **attacked).count());
                        assert_eq!(
                            super::attack_rank(kind, square, side, target),
                            expected,
                            "rank mismatch for kind {kind}, square {square}, side {side}, target {target}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(all(test, valox_nnue_weights))]
mod tests {
    use super::{
        l1_forward_scalar, l2_forward_scalar, transform_accumulators_scalar, NnueEvaluator,
        FEATURES, L2, L3, OUTPUT_BUCKETS,
    };
    use crate::position::{parse_uci_move, parse_uci_move_mode, Position};

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_transform_matches_scalar() {
        if !std::arch::is_x86_feature_detected!("avx2") {
            return;
        }
        for sample in 0..64u32 {
            let mut state = 0x9e37_79b9u32 ^ sample.wrapping_mul(0x85eb_ca6b);
            let mut accumulators = [[0i16; FEATURES]; 2];
            for side in &mut accumulators {
                for value in side {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    *value = state as i16;
                }
            }
            for stm in 0..2 {
                // SAFETY: the feature check above matches the helper's requirements.
                let vector = unsafe { super::transform_accumulators_avx2(stm, &accumulators) };
                let scalar = transform_accumulators_scalar(stm, &accumulators);
                assert_eq!(
                    vector, scalar,
                    "SIMD/scalar transform mismatch for sample {sample}, stm {stm}"
                );
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_l1_matches_scalar_for_every_output_bucket() {
        if !std::arch::is_x86_feature_detected!("avx2") {
            return;
        }
        let model = super::model();
        for bucket in 0..OUTPUT_BUCKETS {
            for sample in 0..32u32 {
                let mut state = 0x9e37_79b9u32 ^ (bucket as u32).wrapping_mul(0x85eb_ca6b) ^ sample;
                let mut transformed = [0u8; FEATURES];
                for value in &mut transformed {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    *value = (state >> 24) as u8 & 0x7f;
                }
                // SAFETY: the feature check above matches the helper's requirements.
                let vector = unsafe { super::l1_forward_avx2(model, bucket, &transformed) };
                let scalar = l1_forward_scalar(model, bucket, &transformed);
                for lane in 0..L2 {
                    assert_eq!(
                        vector[lane], scalar[lane],
                        "SIMD/scalar L1 mismatch in bucket {bucket}, sample {sample}, lane {lane}"
                    );
                }
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_l2_matches_scalar_for_every_output_bucket() {
        if !std::arch::is_x86_feature_detected!("avx2")
            || !std::arch::is_x86_feature_detected!("fma")
        {
            return;
        }
        let model = super::model();
        for bucket in 0..OUTPUT_BUCKETS {
            for sample in 0..32u32 {
                let mut state = 0x9e37_79b9u32 ^ (bucket as u32).wrapping_mul(0x85eb_ca6b) ^ sample;
                let mut input = [0.0f32; L2];
                for value in &mut input {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    *value = (state & 0xffff) as f32 / u16::MAX as f32;
                }
                // SAFETY: the feature checks above match the helper's requirements.
                let vector = unsafe { super::l2_forward_avx2(model, bucket, &input) };
                let scalar = l2_forward_scalar(model, bucket, &input);
                for lane in 0..L3 {
                    assert_eq!(
                        vector[lane].to_bits(),
                        scalar[lane].to_bits(),
                        "SIMD/scalar L2 mismatch in bucket {bucket}, sample {sample}, lane {lane}"
                    );
                }
            }
        }
    }

    fn assert_incremental_move_matches_refresh(fen: &str, uci: &str, chess960: bool) {
        let position = Position::from_fen(fen).expect("valid test position");
        let mv = parse_uci_move_mode(&position, uci, chess960).expect("legal test move");
        let mut child = position.clone();
        let undo = child.make_move(mv);
        let mut incremental = NnueEvaluator::default();
        assert!(incremental.reset(&position));
        let parent_score = incremental.evaluate(&position);
        incremental.push(&child, undo);
        let mut refreshed = NnueEvaluator::default();
        assert!(refreshed.reset(&child));
        assert_eq!(
            incremental.evaluate(&child),
            refreshed.evaluate(&child),
            "incremental mismatch after {uci} from {fen}"
        );
        incremental.pop();
        assert_eq!(incremental.evaluate(&position), parent_score);
    }

    #[test]
    fn accumulator_special_moves_match_full_refresh_and_restore_on_pop() {
        assert_incremental_move_matches_refresh("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1", "e4d5", false);
        assert_incremental_move_matches_refresh("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6", false);
        assert_incremental_move_matches_refresh(
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            "e1g1",
            false,
        );
        assert_incremental_move_matches_refresh("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7a8q", false);
        assert_incremental_move_matches_refresh("4k3/8/8/8/8/8/8/R1K4R w HA - 0 1", "c1a1", true);
    }

    #[test]
    fn accumulator_matches_full_refresh_along_deep_random_legal_line_and_unwinds() {
        let mut position = Position::start();
        let mut incremental = NnueEvaluator::default();
        assert!(incremental.reset(&position));
        let mut previous = Vec::with_capacity(128);
        for ply in 0..128usize {
            let moves = position.legal_moves();
            if moves.is_empty() {
                break;
            }
            let index = (position.hash() as usize ^ ply.wrapping_mul(7919)) % moves.len();
            let mut child = position.clone();
            let undo = child.make_move(moves[index]);
            previous.push((position.clone(), incremental.evaluate(&position)));
            incremental.push(&child, undo);
            let mut refreshed = NnueEvaluator::default();
            assert!(refreshed.reset(&child));
            assert_eq!(
                incremental.evaluate(&child),
                refreshed.evaluate(&child),
                "incremental mismatch at ply {} after {}",
                ply + 1,
                moves[index].uci()
            );
            position = child;
        }
        for (parent, score) in previous.into_iter().rev() {
            incremental.pop();
            assert_eq!(incremental.evaluate(&parent), score);
        }
    }

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
                let mut next = position.clone();
                let undo = next.make_move(mv);
                incremental.push(&next, undo);
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
        let mut promoted = position.clone();
        let undo = promoted.make_move(promotion);
        let mut incremental = NnueEvaluator::default();
        assert!(incremental.reset(&position));
        incremental.push(&promoted, undo);
        let mut refreshed = NnueEvaluator::default();
        assert!(refreshed.reset(&promoted));
        assert_eq!(
            incremental.evaluate(&promoted),
            refreshed.evaluate(&promoted)
        );
    }
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_feature_update_matches_scalar() {
        let model = super::model();
        if model.len() != super::MODEL_BYTES || !std::arch::is_x86_feature_detected!("avx2") {
            return;
        }
        for feature in [0usize, 1, 17, 767] {
            for piece in [false, true] {
                for sign in [-1i32, 1] {
                    let mut scalar = [0i16; super::FEATURES];
                    let mut vector = [0i16; super::FEATURES];
                    super::apply_feature_scalar(model, &mut scalar, feature, piece, sign);
                    // SAFETY: AVX2 support was checked above.
                    unsafe { super::apply_feature_avx2(model, &mut vector, feature, piece, sign) };
                    assert_eq!(
                        scalar, vector,
                        "feature {feature} piece {piece} sign {sign}"
                    );
                }
            }
        }
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
