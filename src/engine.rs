use crate::position::{Move, Position};
#[cfg(not(target_arch = "wasm32"))]
use crate::syzygy::{self, ValoxAdapter};
use crate::tt::{Bound, Entry, TranspositionTable};
#[cfg(not(target_arch = "wasm32"))]
use pyrrhic_rs::{TableBases, WdlProbeResult};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::AtomicUsize;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
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

fn format_uci_score(score: i32) -> String {
    if score.abs() >= MATE - 1_000 {
        let moves = ((MATE - score.abs() + 1) / 2).max(1);
        format!("mate {}", moves * score.signum())
    } else {
        format!("cp {score}")
    }
}

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

/// True when `key` already occurred on the current search path with the same
/// side to move. Every move flips the side, so only every second ancestor can
/// match; the repetition hash includes the side to move.
fn path_has_repetition(path: &[u64], key: u64, ply: usize) -> bool {
    let ply = ply.min(path.len());
    if ply < 2 {
        return false;
    }
    let mut index = ply - 2;
    loop {
        if path[index] == key {
            return true;
        }
        if index < 2 {
            return false;
        }
        index -= 2;
    }
}

/// Number of shared transposition-table entries for a UCI `Hash` setting.
pub fn transposition_entries_for_mb(hash_mb: usize) -> usize {
    crate::tt::entries_for_mb(hash_mb)
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
    pub tt: Arc<TranspositionTable>,
    nodes: u64,
    deadline: Instant,
    soft_deadline: Instant,
    stop: Arc<AtomicBool>,
    aborted: bool,
    report: bool,
    uci_chess960: bool,
    generation: u8,
    node_budget: Arc<AtomicU64>,
    node_limit: Option<u64>,
    killers: [[Option<Move>; 2]; 128],
    history: [[i32; 64]; 64],
    continuation: HashMap<u32, i16>,
    previous_moves: [Option<Move>; 128],
    path: [u64; 128],
    nnue: crate::nnue::NnueEvaluator,
    order_scores: [(i32, Move, i32); 256],
    #[cfg(not(target_arch = "wasm32"))]
    tablebases: Option<Arc<TableBases<ValoxAdapter>>>,
}

impl Searcher {
    pub fn new(
        tt: Arc<TranspositionTable>,
        stop: Arc<AtomicBool>,
        time: Duration,
        report: bool,
    ) -> Self {
        Self::new_with_history(tt, stop, time, SearchHistory::default(), report)
    }

    pub fn new_with_history(
        tt: Arc<TranspositionTable>,
        stop: Arc<AtomicBool>,
        time: Duration,
        search_history: SearchHistory,
        report: bool,
    ) -> Self {
        Self {
            tt,
            nodes: 0,
            deadline: Instant::now() + time,
            soft_deadline: Instant::now() + time,
            stop,
            aborted: false,
            report,
            uci_chess960: false,
            generation: NEXT_TT_GENERATION.fetch_add(1, Ordering::Relaxed),
            node_budget: Arc::new(AtomicU64::new(0)),
            node_limit: None,
            killers: search_history.killers,
            history: search_history.history,
            continuation: search_history.continuation,
            previous_moves: [None; 128],
            path: [0; 128],
            nnue: crate::nnue::NnueEvaluator::default(),
            order_scores: [(i32::MIN, Move::default(), 0); 256],
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

    pub fn set_node_limit(&mut self, limit: Option<u64>) {
        self.node_limit = limit.filter(|value| *value > 0);
        self.node_budget.store(0, Ordering::Relaxed);
    }

    /// Start no new iteration after `time`, but allow an iteration that is
    /// already running to finish until the hard `deadline` set at construction.
    pub fn set_soft_time(&mut self, time: Duration) {
        self.soft_deadline = Instant::now() + time;
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
        path_has_repetition(&self.path, key, ply.max(0) as usize)
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
        let node_limit_reached = self
            .node_limit
            .is_some_and(|limit| self.node_budget.load(Ordering::Relaxed) >= limit);
        let time_or_stop = self.nodes & 511 == 0
            && (self.stop.load(Ordering::Relaxed) || Instant::now() >= self.deadline);
        if node_limit_reached || time_or_stop {
            self.aborted = true;
        }
        self.aborted
    }

    fn generate_moves(position: &Position) -> ([Move; 256], usize) {
        let mut moves = [Move::default(); 256];
        let len = position.legal_moves_into(&mut moves);
        (moves, len)
    }

    fn generate_captures(position: &Position) -> ([Move; 256], usize) {
        let mut moves = [Move::default(); 256];
        let len = position.legal_captures_into(&mut moves);
        (moves, len)
    }

    fn order_moves(
        &mut self,
        position: &Position,
        moves: &mut [Move],
        tt_move: Option<Move>,
        ply: i32,
        see_scores: Option<&mut [i32; 256]>,
    ) {
        let mut see_scores = see_scores;
        let killers = self
            .killers
            .get(ply.max(0) as usize)
            .copied()
            .unwrap_or([None; 2]);
        for (index, mv) in moves.iter().copied().enumerate() {
            let mut see = 0;
            let score = if Some(mv) == tt_move {
                i32::MAX
            } else if position.is_capture(mv) || mv.promotion.is_some() {
                see = position.static_exchange_eval(mv);
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
            self.order_scores[index] = (score, mv, see);
        }
        self.order_scores[..moves.len()].sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
        for (index, (mv, (_, ordered, see))) in
            moves.iter_mut().zip(self.order_scores.iter()).enumerate()
        {
            *mv = *ordered;
            if let Some(scores) = &mut see_scores {
                scores[index] = *see;
            }
        }
    }

    fn negamax(
        &mut self,
        position: &mut Position,
        mut depth: i32,
        mut alpha: i32,
        beta: i32,
        ply: i32,
        allow_null: bool,
    ) -> i32 {
        self.nodes += 1;
        if self.node_limit.is_some() {
            self.node_budget.fetch_add(1, Ordering::Relaxed);
        }
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
        let in_check = position.in_check(position.side);
        if depth <= 0 {
            return self.quiescence(position, alpha, beta, ply, 0);
        }
        if in_check && ply < 32 && depth <= 8 {
            depth += 1;
        }

        let (key, repetition_key) = position.search_hashes();
        if self.repeated_position(repetition_key, ply) {
            return 0;
        }
        self.remember_position(repetition_key, ply);
        let original_alpha = alpha;
        let mut tt_move = None;
        let mut tt_eval = None;
        if let Some(entry) = self.tt.probe(key) {
            tt_move = entry.best();
            tt_eval = entry.eval();
            if entry.depth() >= depth {
                let tt_score = score_from_tt(entry.score(), ply);
                match entry.bound() {
                    Bound::Exact => return tt_score,
                    Bound::Lower if tt_score >= beta => return tt_score,
                    Bound::Upper if tt_score <= alpha => return tt_score,
                    _ => {}
                }
            }
        }
        let (mut move_buffer, move_count) = Self::generate_moves(position);
        if move_count == 0 {
            return if in_check { -MATE + ply } else { 0 };
        }
        // Every consumer of the static evaluation below requires a quiet
        // position, so do not pay for the NNUE forward pass when in check.
        // A shallow TT entry already carries the exact static evaluation.
        let static_eval = if in_check {
            0
        } else if let Some(eval) = tt_eval {
            eval
        } else {
            self.evaluate_position(position)
        };
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
        self.order_moves(position, &mut move_buffer[..move_count], tt_move, ply, None);
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
            self.nnue.push(position, undo);
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
                if quiet {
                    let slot = ply.clamp(0, 127) as usize;
                    if self.killers[slot][0] != Some(mv) {
                        self.killers[slot][1] = self.killers[slot][0];
                        self.killers[slot][0] = Some(mv);
                    }
                    let bonus = (depth * depth * 8).min(2_048);
                    self.update_history(mv, bonus);
                    self.update_continuation(self.previous_moves[slot], mv, bonus);
                    for previous in move_buffer[..move_index].iter().copied() {
                        if !position.is_capture(previous) && previous.promotion.is_none() {
                            self.update_history(previous, -bonus / 2);
                            self.update_continuation(
                                self.previous_moves[slot],
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
        self.tt.store(
            key,
            Entry::new(
                depth,
                score_to_tt(best_score, ply),
                bound,
                best_move,
                self.generation & 0x3f,
                (!in_check).then_some(static_eval),
            ),
        );
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
        if self.node_limit.is_some() {
            self.node_budget.fetch_add(1, Ordering::Relaxed);
        }
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
                let mut child = position.clone();
                let undo = child.make_move(mv);
                self.nnue.push(&child, undo);
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
        let (mut move_buffer, move_count) = if in_check {
            Self::generate_moves(position)
        } else {
            Self::generate_captures(position)
        };
        if move_count == 0 {
            if in_check {
                return -MATE + ply;
            }
            // No captures: `alpha` already includes stand pat. The full
            // generator returned 0 only for stalemate, so distinguish that
            // rare case instead of treating every quiet position as a draw.
            let mut scratch = [Move::default(); 256];
            if position.legal_moves_into(&mut scratch) == 0 {
                return 0;
            }
            return alpha;
        }
        let mut see_scores = [0i32; 256];
        self.order_moves(
            position,
            &mut move_buffer[..move_count],
            None,
            ply,
            Some(&mut see_scores),
        );
        for (move_index, mv) in move_buffer[..move_count].iter().copied().enumerate() {
            let is_capture = position.is_capture(mv);
            let see = see_scores[move_index];
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
            self.nnue.push(position, undo);
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
            if self.stop.load(Ordering::Relaxed) || Instant::now() >= self.soft_deadline {
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
                self.order_moves(&root, &mut move_buffer[..move_count], depth_best, 0, None);
                let mut local_alpha = alpha;
                depth_score = -MATE;
                depth_best = best;
                for (move_index, mv) in move_buffer[..move_count].iter().copied().enumerate() {
                    let undo = root.make_move(mv);
                    self.nnue.push(&root, undo);
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
                    "info depth {depth} score {} nodes {} time {elapsed} nps {nps} pv {}",
                    format_uci_score(depth_score),
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
        self.nnue.reset(position);
        self.path[0] = position.repetition_hash();
        let mut moves = root_moves[..root_len].to_vec();
        let mut last_scores: HashMap<Move, i32> = HashMap::new();
        let started = Instant::now();
        let mut best = moves[0];
        let mut previous_score = 0;
        for depth in 1..=max_depth.max(1) {
            if self.stop.load(Ordering::Relaxed) || Instant::now() >= self.soft_deadline {
                break;
            }
            if let Some(index) = moves.iter().position(|mv| *mv == best) {
                moves.swap(0, index);
            }
            if depth > 1 {
                moves[1..].sort_unstable_by_key(|mv| {
                    std::cmp::Reverse(last_scores.get(mv).copied().unwrap_or(i32::MIN))
                });
            }

            // Search the previous best move first with an aspiration window.
            // Its exact score becomes the threshold for the remaining moves.
            let mut baseline_score = previous_score;
            let mut baseline_exact = false;
            let mut window = if depth >= 4 { 50 } else { MATE };
            loop {
                let alpha = if window == MATE {
                    -MATE
                } else {
                    (previous_score - window).max(-MATE)
                };
                let beta = if window == MATE {
                    MATE
                } else {
                    (previous_score + window).min(MATE)
                };
                let mut child = position.play(best);
                self.nnue.reset(&child);
                self.previous_moves[1] = Some(best);
                let score = -self.negamax(&mut child, depth as i32 - 1, -beta, -alpha, 1, true);
                if self.aborted {
                    break;
                }
                if score <= alpha && alpha > -MATE {
                    window = (window * 2).min(MATE);
                    continue;
                }
                if score >= beta && beta < MATE {
                    window = (window * 2).min(MATE);
                    continue;
                }
                baseline_score = score;
                baseline_exact = true;
                break;
            }
            if !baseline_exact {
                break;
            }

            // The main thread already searched the previous best move with a
            // full window; that established the threshold. It then waits for
            // the workers, so all requested threads can pull candidate moves
            // from the shared queue during the parallel phase. This keeps the
            // fourth core busy instead of leaving it idle with 3 workers.
            let candidates = &moves[1..];
            let worker_count = threads.min(candidates.len()).max(1);
            let next_candidate = AtomicUsize::new(0);
            let time_left = self.deadline.saturating_duration_since(Instant::now());
            let uci_chess960 = self.uci_chess960;
            let node_budget = self.node_budget.clone();
            let node_limit = self.node_limit;
            let generation = self.generation;
            let threshold = baseline_score;
            let results = std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(worker_count);
                for _ in 0..worker_count {
                    if candidates.is_empty() {
                        break;
                    }
                    let stop = self.stop.clone();
                    let history = self.history_state();
                    let worker_node_budget = node_budget.clone();
                    let worker_tt = self.tt.clone();
                    let worker_candidates = candidates;
                    let worker_queue = &next_candidate;
                    #[cfg(not(target_arch = "wasm32"))]
                    let tablebases = self.tablebases.clone();
                    handles.push(scope.spawn(move || {
                        let mut worker =
                            Searcher::new_with_history(worker_tt, stop, time_left, history, false);
                        worker.node_budget = worker_node_budget;
                        worker.node_limit = node_limit;
                        worker.generation = generation;
                        worker.set_uci_chess960(uci_chess960);
                        worker.path[0] = position.repetition_hash();
                        #[cfg(not(target_arch = "wasm32"))]
                        worker.set_tablebases(tablebases);
                        let mut scores = Vec::with_capacity(worker_candidates.len());
                        let mut complete = true;
                        loop {
                            let index = worker_queue.fetch_add(1, Ordering::Relaxed);
                            let Some(&mv) = worker_candidates.get(index) else {
                                break;
                            };
                            if worker.stop.load(Ordering::Relaxed)
                                || Instant::now() >= worker.deadline
                            {
                                complete = false;
                                break;
                            }
                            let mut child = position.play(mv);
                            worker.nnue.reset(&child);
                            worker.previous_moves[1] = Some(mv);
                            // Null-window test: is this move better than the
                            // current best? Re-search only on a fail-high.
                            let mut score = -worker.negamax(
                                &mut child,
                                depth as i32 - 1,
                                -threshold - 1,
                                -threshold,
                                1,
                                true,
                            );
                            if worker.aborted {
                                complete = false;
                                break;
                            }
                            if score > threshold {
                                score = -worker.negamax(
                                    &mut child,
                                    depth as i32 - 1,
                                    -MATE,
                                    -threshold,
                                    1,
                                    true,
                                );
                                if worker.aborted {
                                    complete = false;
                                    break;
                                }
                            }
                            scores.push((mv, score, score > threshold));
                        }
                        (scores, worker, complete)
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
            let mut complete = true;
            let mut best_score = baseline_score;
            let mut depth_best = best;
            last_scores.clear();
            last_scores.insert(best, baseline_score);
            for (scores, worker, worker_complete) in &results {
                self.nodes = self.nodes.saturating_add(worker.nodes);
                complete &= *worker_complete;
                for (mv, score, better) in scores {
                    if *better && *score > best_score {
                        best_score = *score;
                        depth_best = *mv;
                    }
                    last_scores.insert(*mv, *score);
                }
            }
            if let Some((_, worker, _)) = results.first() {
                let state = worker.history_state();
                self.history = state.history;
                self.killers = state.killers;
                self.continuation = state.continuation;
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
                    "info depth {depth} score {} nodes {} time {elapsed} nps {nps} pv {}",
                    format_uci_score(best_score),
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
        Arc::new(TranspositionTable::new(65_536)),
        signal,
        Duration::from_secs(60),
        false,
    );
    searcher.search(position, depth)
}

#[cfg(test)]
mod tests {
    use super::{
        format_uci_score, path_has_repetition, transposition_entries_for_mb, Searcher, MATE,
    };
    use crate::position::Position;
    use crate::tt::TranspositionTable;
    use std::sync::{atomic::AtomicBool, Arc};
    use std::time::Duration;

    fn shared_table() -> Arc<TranspositionTable> {
        Arc::new(TranspositionTable::new(4096))
    }

    #[test]
    fn repetition_scan_checks_only_same_side_ancestors() {
        let path = [11, 22, 33, 44, 55];
        // Ply 4 has the same side to move as plies 2 and 0.
        assert!(path_has_repetition(&path, 33, 4));
        assert!(path_has_repetition(&path, 11, 4));
        assert!(!path_has_repetition(&path, 22, 4));
        assert!(!path_has_repetition(&path, 44, 4));
        // Ply 3 has the same side to move as ply 1.
        assert!(path_has_repetition(&path, 22, 3));
        assert!(!path_has_repetition(&path, 33, 3));
        // The current position is not on the path yet.
        assert!(!path_has_repetition(&path, 55, 4));
        assert!(!path_has_repetition(&path, 11, 1));
        assert!(!path_has_repetition(&path, 11, 0));
        // A ply beyond the recorded path is clamped safely.
        assert!(path_has_repetition(&path, 22, 99));
    }

    #[test]
    fn hash_option_scales_to_requested_memory() {
        assert_eq!(transposition_entries_for_mb(16), 1_048_576);
        assert_eq!(transposition_entries_for_mb(16) * 16, 16 * 1024 * 1024);
        assert_eq!(transposition_entries_for_mb(256) * 16, 256 * 1024 * 1024);
        assert_eq!(transposition_entries_for_mb(0), 1);
    }

    #[test]
    fn transposition_entry_fits_in_one_slot_word() {
        assert_eq!(std::mem::size_of::<crate::tt::Entry>(), 8);
        assert_eq!(std::mem::size_of::<TranspositionTable>(), 32);
    }

    #[test]
    fn mate_scores_use_uci_mate_distance_not_centipawns() {
        assert_eq!(format_uci_score(MATE - 1), "mate 1");
        assert_eq!(format_uci_score(-MATE + 2), "mate -1");
        assert_eq!(format_uci_score(MATE - 4), "mate 2");
        assert_eq!(format_uci_score(123), "cp 123");
    }

    #[test]
    fn node_limit_stops_search_at_requested_budget() {
        let position = Position::start();
        let mut searcher = Searcher::new(
            shared_table(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(60),
            false,
        );
        searcher.set_node_limit(Some(10_000));
        let best = searcher.search(&position, 64).expect("legal root move");
        assert!(position.legal_moves().contains(&best));
        assert!(searcher.nodes() <= 10_000);
        assert!(searcher.nodes() >= 9_000);
    }

    #[test]
    fn threaded_search_finds_mate_in_one() {
        let position =
            Position::from_fen("6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1").expect("valid FEN");
        let mut searcher = Searcher::new(
            shared_table(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(30),
            false,
        );
        let best = searcher
            .search_with_threads(&position, 3, 4)
            .expect("legal root move");
        assert_eq!(best.uci(), "a1a8");
        assert!(position.legal_moves().contains(&best));
    }

    #[test]
    fn threaded_search_returns_a_legal_move() {
        let position = Position::start();
        let mut searcher = Searcher::new(
            shared_table(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(30),
            false,
        );
        let best = searcher
            .search_with_threads(&position, 4, 4)
            .expect("legal root move");
        assert!(position.legal_moves().contains(&best));
    }
}
