use crate::position::{Move, Position};
#[cfg(not(target_arch = "wasm32"))]
use crate::syzygy::{self, ValoxAdapter};
#[cfg(not(target_arch = "wasm32"))]
use pyrrhic_rs::{TableBases, WdlProbeResult};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Arc,
};
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Instant(f64);
#[cfg(target_arch = "wasm32")]
impl Instant {
    fn now() -> Self {
        Self(unsafe { valox_now_ms() })
    }
    fn elapsed(self) -> Duration {
        Duration::from_secs_f64(((unsafe { valox_now_ms() }) - self.0).max(0.0) / 1000.0)
    }
}
#[cfg(target_arch = "wasm32")]
impl std::ops::Add<Duration> for Instant {
    type Output = Self;
    fn add(self, rhs: Duration) -> Self {
        Self(self.0 + rhs.as_secs_f64() * 1000.0)
    }
}
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn valox_now_ms() -> f64;
}
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

const MATE: i32 = 30_000;
static NEXT_TT_GENERATION: AtomicU8 = AtomicU8::new(0);

fn score_to_tt(score: i32, ply: i32) -> i32 {
    if score > MATE - 1_000 {
        score + ply
    } else if score < -MATE + 1_000 {
        score - ply
    } else {
        score
    }
}

fn score_from_tt(score: i32, ply: i32) -> i32 {
    if score > MATE - 1_000 {
        score - ply
    } else if score < -MATE + 1_000 {
        score + ply
    } else {
        score
    }
}

#[derive(Clone, Copy)]
enum Bound {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy)]
pub struct Entry {
    depth: i32,
    score: i32,
    bound: Bound,
    best: Option<Move>,
    age: u8,
}

/// Estimate a safe transposition-table entry limit for a requested Hash size.
/// HashMap needs space for the key, entry, control bytes and capacity rounding;
/// the factor of two keeps its allocation near (and below) the UCI MB setting.
pub fn transposition_entries_for_mb(hash_mb: usize) -> usize {
    let bytes = hash_mb.saturating_mul(1024 * 1024);
    let bytes_per_entry = std::mem::size_of::<(u64, Entry)>().saturating_mul(2).max(1);
    (bytes / bytes_per_entry).max(1)
}

#[derive(Clone)]
pub struct SearchHistory {
    killers: [[Option<Move>; 2]; 128],
    history: [[i32; 64]; 64],
    continuation: HashMap<u32, i16>,
}

impl Default for SearchHistory {
    fn default() -> Self {
        Self {
            killers: [[None; 2]; 128],
            history: [[0; 64]; 64],
            continuation: HashMap::new(),
        }
    }
}

pub struct Searcher {
    pub tt: HashMap<u64, Entry>,
    tt_limit: usize,
    nodes: u64,
    deadline: Instant,
    stop: Arc<AtomicBool>,
    aborted: bool,
    report: bool,
    uci_chess960: bool,
    generation: u8,
    killers: [[Option<Move>; 2]; 128],
    history: [[i32; 64]; 64],
    continuation: HashMap<u32, i16>,
    previous_moves: [Option<Move>; 128],
    path: [u64; 128],
    nnue: crate::nnue::NnueEvaluator,
    #[cfg(not(target_arch = "wasm32"))]
    tablebases: Option<Arc<TableBases<ValoxAdapter>>>,
}

impl Searcher {
    pub fn new(
        tt: HashMap<u64, Entry>,
        tt_limit: usize,
        stop: Arc<AtomicBool>,
        time: Duration,
        report: bool,
    ) -> Self {
        Self::new_with_history(tt, tt_limit, stop, time, SearchHistory::default(), report)
    }

    pub fn new_with_history(
        tt: HashMap<u64, Entry>,
        tt_limit: usize,
        stop: Arc<AtomicBool>,
        time: Duration,
        search_history: SearchHistory,
        report: bool,
    ) -> Self {
        Self {
            tt,
            tt_limit,
            nodes: 0,
            deadline: Instant::now() + time,
            stop,
            aborted: false,
            report,
            uci_chess960: false,
            generation: NEXT_TT_GENERATION.fetch_add(1, Ordering::Relaxed),
            killers: search_history.killers,
            history: search_history.history,
            continuation: search_history.continuation,
            previous_moves: [None; 128],
            path: [0; 128],
            nnue: crate::nnue::NnueEvaluator::default(),
            #[cfg(not(target_arch = "wasm32"))]
            tablebases: None,
        }
    }

    pub fn history_state(&self) -> SearchHistory {
        SearchHistory {
            killers: self.killers,
            history: self.history,
            continuation: self.continuation.clone(),
        }
    }

    pub fn nodes(&self) -> u64 {
        self.nodes
    }

    pub fn set_uci_chess960(&mut self, enabled: bool) {
        self.uci_chess960 = enabled;
    }

    fn evaluate_position(&mut self, position: &Position) -> i32 {
        self.nnue
            .evaluate(position)
            .unwrap_or_else(|| position.evaluate_classical())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_tablebases(&mut self, tablebases: Option<Arc<TableBases<ValoxAdapter>>>) {
        self.tablebases = tablebases;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn tablebase_score(&self, position: &Position, ply: i32) -> Option<i32> {
        let table = self.tablebases.as_ref()?;
        match syzygy::probe(table, position)? {
            WdlProbeResult::Win => Some(MATE - ply),
            WdlProbeResult::Loss => Some(-MATE + ply),
            WdlProbeResult::Draw | WdlProbeResult::CursedWin | WdlProbeResult::BlessedLoss => {
                Some(0)
            }
        }
    }

    fn repeated_position(&self, key: u64, ply: i32) -> bool {
        self.path
            .iter()
            .take((ply.max(0) as usize).min(self.path.len()))
            .any(|&previous| previous == key)
    }

    fn remember_position(&mut self, key: u64, ply: i32) {
        if let Some(slot) = self.path.get_mut(ply.max(0) as usize) {
            *slot = key;
        }
    }

    fn update_history(&mut self, mv: Move, bonus: i32) {
        const HISTORY_LIMIT: i32 = 16_384;
        let value = &mut self.history[mv.from as usize][mv.to as usize];
        let bonus = bonus.clamp(-2_048, 2_048);
        *value = (*value + bonus - *value * bonus.abs() / HISTORY_LIMIT)
            .clamp(-HISTORY_LIMIT, HISTORY_LIMIT);
    }

    fn continuation_key(previous: Move, current: Move) -> u32 {
        ((previous.from as u32) << 18)
            | ((previous.to as u32) << 12)
            | ((current.from as u32) << 6)
            | current.to as u32
    }

    fn update_continuation(&mut self, previous: Option<Move>, current: Move, bonus: i32) {
        if let Some(previous) = previous {
            let value = self
                .continuation
                .entry(Self::continuation_key(previous, current))
                .or_default();
            *value = (*value as i32 + bonus - *value as i32 * bonus.abs() / 16_384)
                .clamp(-16_384, 16_384) as i16;
        }
    }

    fn should_stop(&mut self) -> bool {
        if self.nodes & 2047 == 0
            && (self.stop.load(Ordering::Relaxed) || Instant::now() >= self.deadline)
        {
            self.aborted = true;
        }
        self.aborted
    }

    fn generate_moves(position: &Position) -> ([Move; 256], usize) {
        let mut moves = [Move::default(); 256];
        let len = position.legal_moves_into(&mut moves);
        (moves, len)
    }

    fn order_moves(
        &self,
        position: &Position,
        moves: &mut [Move],
        tt_move: Option<Move>,
        ply: i32,
    ) {
        let killers = self
            .killers
            .get(ply.max(0) as usize)
            .copied()
            .unwrap_or([None; 2]);
        let mut scored = [(i32::MIN, Move::default()); 256];
        for (index, mv) in moves.iter().copied().enumerate() {
            let score = if Some(mv) == tt_move {
                i32::MAX
            } else if position.is_capture(mv) || mv.promotion.is_some() {
                let see = position.static_exchange_eval(mv);
                if see >= 0 {
                    1_200_000 + see
                } else {
                    // Search profitable exchanges before quiet moves. Push losing
                    // captures behind killers/history so likely refutations come first.
                    -100_000 + see
                }
            } else if killers[0] == Some(mv) {
                1_000_000
            } else if killers[1] == Some(mv) {
                900_000
            } else {
                self.history[mv.from as usize][mv.to as usize]
                    + self
                        .previous_moves
                        .get(ply.max(0) as usize)
                        .and_then(|previous| *previous)
                        .and_then(|previous| {
                            self.continuation
                                .get(&Self::continuation_key(previous, mv))
                                .copied()
                        })
                        .unwrap_or(0) as i32
                        * 2
            };
            scored[index] = (score, mv);
        }
        scored[..moves.len()].sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
        for (mv, (_, ordered)) in moves.iter_mut().zip(scored.iter()) {
            *mv = *ordered;
        }
    }

    fn negamax(
        &mut self,
        position: &mut Position,
        depth: i32,
        mut alpha: i32,
        beta: i32,
        ply: i32,
        allow_null: bool,
    ) -> i32 {
        self.nodes += 1;
        if self.should_stop() {
            return self.evaluate_position(position);
        }
        if position.is_fifty_move_draw() {
            if position.in_check(position.side) {
                let (moves, len) = Self::generate_moves(position);
                if len == 0 {
                    return -MATE + ply;
                }
                let _ = moves;
            }
            return 0;
        }
        if position.is_insufficient_material() {
            return 0;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(score) = self.tablebase_score(position, ply) {
            return score;
        }
        if depth <= 0 {
            return self.quiescence(position, alpha, beta, ply, 0);
        }

        let key = position.hash();
        let repetition_key = position.repetition_hash();
        if self.repeated_position(repetition_key, ply) {
            return 0;
        }
        self.remember_position(repetition_key, ply);
        let original_alpha = alpha;
        let mut tt_move = None;
        if let Some(entry) = self.tt.get(&key).copied() {
            tt_move = entry.best;
            if entry.depth >= depth {
                let tt_score = score_from_tt(entry.score, ply);
                match entry.bound {
                    Bound::Exact => return tt_score,
                    Bound::Lower if tt_score >= beta => return tt_score,
                    Bound::Upper if tt_score <= alpha => return tt_score,
                    _ => {}
                }
            }
        }
        let in_check = position.in_check(position.side);
        let (mut move_buffer, move_count) = Self::generate_moves(position);
        if move_count == 0 {
            return if in_check { -MATE + ply } else { 0 };
        }
        let static_eval = self.evaluate_position(position);
        if depth <= 2
            && beta - alpha <= 1
            && !in_check
            && beta.abs() < MATE - 1_000
            && static_eval + 180 * depth <= alpha
        {
            let razor_score = self.quiescence(position, alpha, beta, ply, 0);
            if self.aborted {
                return 0;
            }
            if razor_score <= alpha {
                return razor_score;
            }
        }
        // Reverse futility is limited to shallow, non-PV nodes. At those
        // nodes a large static advantage is enough to avoid a broad subtree.
        if depth <= 3
            && beta - alpha <= 1
            && !in_check
            && beta.abs() < MATE - 1_000
            && static_eval - (90 + depth * 85) >= beta
        {
            return static_eval;
        }

        // Null-move pruning quickly rejects quiet positions where the side to
        // move has enough major material to avoid common zugzwang failures.
        if allow_null
            && depth >= 3
            && !in_check
            && beta.abs() < MATE - 100
            && position.non_pawn_material(position.side) >= 800
            && position.fifty_move_clock() < 90
        {
            let reduction = 2 + depth / 5 + ((static_eval - beta).max(0) / 200).min(2);
            let mut null_position = position.null_move();
            self.previous_moves[(ply + 1).clamp(0, 127) as usize] = None;
            let score = -self.negamax(
                &mut null_position,
                depth - 1 - reduction,
                -beta,
                -beta + 1,
                ply + 1,
                true,
            );
            if self.aborted {
                return 0;
            }
            if score >= beta {
                if depth < 7 {
                    return score;
                }
                let verified =
                    self.negamax(position, depth - 1 - reduction, beta - 1, beta, ply, false);
                if self.aborted {
                    return 0;
                }
                if verified >= beta {
                    return verified;
                }
            }
        }
        self.order_moves(position, &mut move_buffer[..move_count], tt_move, ply);
        let mut best_score = -MATE;
        let mut best_move = None;
        for (move_index, mv) in move_buffer[..move_count].iter().copied().enumerate() {
            // Read capture status while the original destination is still on the board.
            // After make_move, the destination contains the moving piece and looks captured.
            let quiet = !position.is_capture(mv) && mv.promotion.is_none();
            let undo = position.make_move(mv);
            self.previous_moves[(ply + 1).clamp(0, 127) as usize] = Some(mv);
            let gives_check = position.in_check(position.side);
            if !in_check && !gives_check && quiet && beta - alpha <= 1 {
                if depth <= 3 && move_index >= (4 + depth * depth) as usize {
                    position.unmake_move(undo);
                    continue;
                }
                if depth <= 2 && move_index > 0 && static_eval + 100 * depth + 70 <= alpha {
                    position.unmake_move(undo);
                    continue;
                }
            }
            let reduction = if depth >= 3 && move_index >= 3 && quiet && !in_check && !gives_check {
                (1 + depth / 6 + move_index as i32 / 8)
                    .min(depth - 2)
                    .max(1)
            } else {
                0
            };
            let reducible = reduction > 0;
            self.nnue.push(position);
            let mut score = if move_index == 0 {
                -self.negamax(position, depth - 1, -beta, -alpha, ply + 1, true)
            } else {
                -self.negamax(
                    position,
                    depth - 1 - reduction,
                    -alpha - 1,
                    -alpha,
                    ply + 1,
                    true,
                )
            };
            if self.aborted {
                self.nnue.pop();
                position.unmake_move(undo);
                return 0;
            }
            if reducible && score > alpha {
                score = -self.negamax(position, depth - 1, -alpha - 1, -alpha, ply + 1, true);
            }
            if move_index > 0 && score > alpha && score < beta {
                score = -self.negamax(position, depth - 1, -beta, -alpha, ply + 1, true);
            }
            self.nnue.pop();
            position.unmake_move(undo);
            if self.aborted {
                return 0;
            }
            if score > best_score {
                best_score = score;
                best_move = Some(mv);
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                if !position.is_capture(mv) && mv.promotion.is_none() {
                    let slot = ply.clamp(0, 127) as usize;
                    if self.killers[slot][0] != Some(mv) {
                        self.killers[slot][1] = self.killers[slot][0];
                        self.killers[slot][0] = Some(mv);
                    }
                    let bonus = (depth * depth * 8).min(2_048);
                    self.update_history(mv, bonus);
                    self.update_continuation(
                        self.previous_moves[ply.clamp(0, 127) as usize],
                        mv,
                        bonus,
                    );
                    for previous in move_buffer[..move_index].iter().copied() {
                        if !position.is_capture(previous) && previous.promotion.is_none() {
                            self.update_history(previous, -bonus / 2);
                            self.update_continuation(
                                self.previous_moves[ply.clamp(0, 127) as usize],
                                previous,
                                -bonus / 2,
                            );
                        }
                    }
                }
                break;
            }
        }
        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        let replace = match self.tt.get(&key) {
            Some(old) => depth >= old.depth || matches!(bound, Bound::Exact),
            None if self.tt.len() < self.tt_limit.max(1) => true,
            None => {
                // Keep the table useful after it fills instead of freezing
                // it forever. Sample a few entries and replace an old or
                // shallower result when this search found a better one.
                let victim = self
                    .tt
                    .iter()
                    .take(16)
                    .map(|(old_key, old)| {
                        (*old_key, old.depth, self.generation.wrapping_sub(old.age))
                    })
                    .min_by_key(|(_, old_depth, age)| *old_depth - i32::from(*age) * 2);
                if let Some((old_key, old_depth, age)) = victim {
                    if age >= 8 || depth >= old_depth {
                        self.tt.remove(&old_key);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };
        if replace {
            self.tt.insert(
                key,
                Entry {
                    depth,
                    score: score_to_tt(best_score, ply),
                    bound,
                    best: best_move,
                    age: self.generation,
                },
            );
        }
        best_score
    }

    fn quiescence(
        &mut self,
        position: &mut Position,
        mut alpha: i32,
        beta: i32,
        ply: i32,
        qply: u8,
    ) -> i32 {
        self.nodes += 1;
        if self.should_stop() {
            return self.evaluate_position(position);
        }
        let in_check = position.in_check(position.side);
        if position.is_fifty_move_draw() {
            if in_check {
                let (moves, len) = Self::generate_moves(position);
                if len == 0 {
                    return -MATE + ply;
                }
                let _ = moves;
            }
            return 0;
        }
        if position.is_insufficient_material() {
            return 0;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(score) = self.tablebase_score(position, ply) {
            return score;
        }
        let key = position.repetition_hash();
        if self.repeated_position(key, ply) {
            return 0;
        }
        self.remember_position(key, ply);
        if qply >= 12 && !in_check {
            return self.evaluate_position(position);
        }
        if qply >= 12 && in_check {
            let (moves, len) = Self::generate_moves(position);
            if len == 0 {
                return -MATE + ply;
            }
            let mut best = -MATE + ply;
            for mv in moves[..len].iter().copied() {
                let child = position.play(mv);
                self.nnue.push(&child);
                best = best.max(-self.evaluate_position(&child));
                self.nnue.pop();
            }
            return best;
        }
        let stand_pat = self.evaluate_position(position);
        if !in_check {
            if stand_pat >= beta {
                return beta;
            }
            alpha = alpha.max(stand_pat);
        }
        let (mut move_buffer, mut move_count) = Self::generate_moves(position);
        if move_count == 0 {
            return if in_check { -MATE + ply } else { 0 };
        }
        if !in_check {
            let mut kept = 0;
            for read in 0..move_count {
                let mv = move_buffer[read];
                if position.is_capture(mv) || mv.promotion.is_some() {
                    move_buffer[kept] = mv;
                    kept += 1;
                }
            }
            move_count = kept;
        }
        self.order_moves(position, &mut move_buffer[..move_count], None, ply);
        for mv in move_buffer[..move_count].iter().copied() {
            let is_capture = position.is_capture(mv);
            let see = if !in_check && is_capture && mv.promotion.is_none() {
                position.static_exchange_eval(mv)
            } else {
                0
            };
            let undo = position.make_move(mv);
            self.previous_moves[(ply + 1).clamp(0, 127) as usize] = Some(mv);
            if !in_check
                && is_capture
                && mv.promotion.is_none()
                && !position.in_check(position.side)
                && see < -75
                && stand_pat + see + 100 < alpha
            {
                position.unmake_move(undo);
                continue;
            }
            self.nnue.push(position);
            let score = -self.quiescence(position, -beta, -alpha, ply + 1, qply.saturating_add(1));
            self.nnue.pop();
            position.unmake_move(undo);
            if self.aborted {
                return 0;
            }
            if score >= beta {
                return beta;
            }
            alpha = alpha.max(score);
        }
        alpha
    }

    pub fn search(&mut self, position: &Position, max_depth: u32) -> Option<Move> {
        self.nnue.reset(position);
        let mut root = position.clone();
        let (root_moves, root_len) = Self::generate_moves(&root);
        let mut best = root_moves[..root_len].first().copied();
        if root_len == 0 {
            return None;
        }
        self.path[0] = root.repetition_hash();
        let started = Instant::now();
        let mut previous_score = 0;
        for depth in 1..=max_depth.max(1) {
            if self.stop.load(Ordering::Relaxed) || Instant::now() >= self.deadline {
                break;
            }
            let mut window = if depth >= 4 { 40 } else { MATE };
            let mut alpha = if window == MATE {
                -MATE
            } else {
                (previous_score - window).max(-MATE)
            };
            let mut beta = if window == MATE {
                MATE
            } else {
                (previous_score + window).min(MATE)
            };
            let mut depth_best = best;
            let mut depth_score: i32;

            loop {
                let (mut move_buffer, move_count) = Self::generate_moves(&root);
                self.order_moves(&root, &mut move_buffer[..move_count], depth_best, 0);
                let mut local_alpha = alpha;
                depth_score = -MATE;
                depth_best = best;
                for (move_index, mv) in move_buffer[..move_count].iter().copied().enumerate() {
                    let undo = root.make_move(mv);
                    self.nnue.push(&root);
                    self.previous_moves[1] = Some(mv);
                    let mut score = if move_index == 0 {
                        -self.negamax(&mut root, depth as i32 - 1, -beta, -local_alpha, 1, true)
                    } else {
                        -self.negamax(
                            &mut root,
                            depth as i32 - 1,
                            -local_alpha - 1,
                            -local_alpha,
                            1,
                            true,
                        )
                    };
                    if self.aborted {
                        self.nnue.pop();
                        root.unmake_move(undo);
                        break;
                    }
                    if move_index > 0 && score > local_alpha && score < beta {
                        score = -self.negamax(
                            &mut root,
                            depth as i32 - 1,
                            -beta,
                            -local_alpha,
                            1,
                            true,
                        );
                        if self.aborted {
                            self.nnue.pop();
                            root.unmake_move(undo);
                            break;
                        }
                    }
                    self.nnue.pop();
                    root.unmake_move(undo);
                    if score > depth_score {
                        depth_score = score;
                        depth_best = Some(mv);
                    }
                    local_alpha = local_alpha.max(score);
                    if local_alpha >= beta {
                        break;
                    }
                }
                if self.aborted {
                    break;
                }
                if depth_score <= alpha && alpha > -MATE {
                    best = depth_best;
                    window = (window * 2).min(MATE);
                    alpha = (alpha - window).max(-MATE);
                    continue;
                }
                if depth_score >= beta && beta < MATE {
                    best = depth_best;
                    window = (window * 2).min(MATE);
                    beta = (beta + window).min(MATE);
                    continue;
                }
                break;
            }
            if self.aborted {
                break;
            }
            best = depth_best;
            previous_score = depth_score;
            if self.report {
                let elapsed = started.elapsed().as_millis().max(1);
                let nps = self.nodes.saturating_mul(1000) / elapsed as u64;
                println!(
                    "info depth {depth} score cp {depth_score} nodes {} time {elapsed} nps {nps} pv {}",
                    self.nodes,
                    best.map(|m| if self.uci_chess960 {
                        m.uci_chess960()
                    } else {
                        m.uci()
                    })
                    .unwrap_or_default()
                );
            }
            if depth_score.abs() >= MATE - 100 {
                break;
            }
        }
        best
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn search_with_threads(
        &mut self,
        position: &Position,
        max_depth: u32,
        threads: usize,
    ) -> Option<Move> {
        if threads <= 1 {
            return self.search(position, max_depth);
        }
        let (root_moves, root_len) = Self::generate_moves(position);
        if root_len == 0 {
            return None;
        }
        let moves = root_moves[..root_len].to_vec();
        let started = Instant::now();
        let mut best = moves[0];
        let mut previous_score = 0;
        for depth in 1..=max_depth.max(1) {
            if self.stop.load(Ordering::Relaxed) || Instant::now() >= self.deadline {
                break;
            }
            let worker_count = threads.min(moves.len()).max(1);
            let mut partitions = vec![Vec::new(); worker_count];
            for (index, mv) in moves.iter().copied().enumerate() {
                partitions[index % worker_count].push(mv);
            }
            let time_left = self.deadline.saturating_duration_since(Instant::now());
            let tt_limit = (self.tt_limit / worker_count).max(256);
            let worker_tt = self
                .tt
                .iter()
                .take(tt_limit)
                .map(|(key, entry)| (*key, *entry))
                .collect::<HashMap<_, _>>();
            let uci_chess960 = self.uci_chess960;
            let results = std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(worker_count);
                for partition in partitions {
                    let stop = self.stop.clone();
                    let tt = worker_tt.clone();
                    let history = self.history_state();
                    #[cfg(not(target_arch = "wasm32"))]
                    let tablebases = self.tablebases.clone();
                    handles.push(scope.spawn(move || {
                        let mut worker = Searcher::new_with_history(
                            tt, tt_limit, stop, time_left, history, false,
                        );
                        worker.set_uci_chess960(uci_chess960);
                        #[cfg(not(target_arch = "wasm32"))]
                        worker.set_tablebases(tablebases);
                        let mut scores = Vec::with_capacity(partition.len());
                        for mv in partition {
                            if worker.stop.load(Ordering::Relaxed)
                                || Instant::now() >= worker.deadline
                            {
                                break;
                            }
                            let mut child = position.play(mv);
                            worker.nnue.reset(&child);
                            let score =
                                -worker.negamax(&mut child, depth as i32 - 1, -MATE, MATE, 1, true);
                            scores.push((mv, score));
                            if worker.aborted {
                                break;
                            }
                        }
                        (scores, worker)
                    }));
                }
                handles
                    .into_iter()
                    .filter_map(|handle| handle.join().ok())
                    .collect::<Vec<_>>()
            });
            if results.is_empty() {
                break;
            }
            let complete = results
                .iter()
                .map(|(scores, _)| scores.len())
                .sum::<usize>()
                == moves.len();
            let mut best_score = -MATE;
            let mut depth_best = best;
            for (scores, worker) in &results {
                self.nodes = self.nodes.saturating_add(worker.nodes);
                for entry in worker.tt.iter() {
                    if self
                        .tt
                        .get(entry.0)
                        .map(|old| old.depth < entry.1.depth)
                        .unwrap_or(true)
                    {
                        self.tt.insert(*entry.0, *entry.1);
                    }
                }
                for (mv, score) in scores {
                    if *score > best_score {
                        best_score = *score;
                        depth_best = *mv;
                    }
                }
            }
            if let Some((_, worker)) = results.first() {
                self.history = worker.history_state().history;
                self.killers = worker.history_state().killers;
            }
            if !complete {
                break;
            }
            best = depth_best;
            previous_score = best_score;
            if self.report {
                let elapsed = started.elapsed().as_millis().max(1);
                let nps = self.nodes.saturating_mul(1000) / elapsed as u64;
                println!(
                    "info depth {depth} score cp {best_score} nodes {} time {elapsed} nps {nps} pv {}",
                    self.nodes,
                    if self.uci_chess960 {
                        best.uci_chess960()
                    } else {
                        best.uci()
                    }
                );
            }
            if best_score.abs() >= MATE - 100 {
                break;
            }
        }
        let _ = previous_score;
        Some(best)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn search_with_threads(
        &mut self,
        position: &Position,
        max_depth: u32,
        _threads: usize,
    ) -> Option<Move> {
        self.search(position, max_depth)
    }
}

pub fn best_move(position: &Position, depth: u32) -> Option<Move> {
    let signal = Arc::new(AtomicBool::new(false));
    let mut searcher = Searcher::new(
        HashMap::new(),
        65_536,
        signal,
        Duration::from_secs(60),
        false,
    );
    searcher.search(position, depth)
}

#[cfg(test)]
mod tests {
    use super::transposition_entries_for_mb;

    #[test]
    fn hash_option_scales_to_requested_memory() {
        assert!(transposition_entries_for_mb(16) > 2048 * 16);
        let sixteen_mb = transposition_entries_for_mb(16) * 16;
        let two_hundred_fifty_six_mb = transposition_entries_for_mb(256);
        assert!(two_hundred_fifty_six_mb >= sixteen_mb);
        assert!(two_hundred_fifty_six_mb - sixteen_mb < 16);
        assert_eq!(transposition_entries_for_mb(0), 1);
    }
}
