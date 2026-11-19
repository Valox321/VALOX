use std::hint::black_box;
use std::time::Instant;
use valox_engine::nnue::NnueEvaluator;
use valox_engine::position::Position;

fn main() {
    let mut args = std::env::args().skip(1);
    let dataset = args
        .next()
        .expect("usage: nnue-bench DATASET [EVAL_REPEATS] [UPDATE_REPEATS]");
    let eval_repeats = args
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(128);
    let update_repeats = args
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(8);
    let rows = std::fs::read_to_string(dataset)
        .expect("read NNUE dataset")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (category, fen) = line.split_once('\t').expect("category<TAB>FEN row");
            let position = Position::from_fen(fen).expect("valid benchmark FEN");
            (category.to_owned(), position)
        })
        .collect::<Vec<_>>();
    assert!(!rows.is_empty(), "dataset must contain positions");
    #[cfg(target_arch = "x86_64")]
    println!(
        "cpu_features avx2={} fma={}",
        std::arch::is_x86_feature_detected!("avx2"),
        std::arch::is_x86_feature_detected!("fma")
    );

    let mut evaluator = NnueEvaluator::default();
    let mut checksum = 0i64;
    let started = Instant::now();
    let mut evals = 0usize;
    for (_, position) in &rows {
        assert!(evaluator.reset(position), "NNUE weights unavailable");
        for _ in 0..eval_repeats {
            checksum = checksum.wrapping_add(i64::from(
                black_box(&evaluator)
                    .evaluate(black_box(position))
                    .expect("NNUE evaluation"),
            ));
            evals += 1;
        }
    }
    let eval_seconds = started.elapsed().as_secs_f64();
    println!(
        "nnue_eval positions {} evaluations {} time_ms {:.3} evals_per_sec {:.1} us_per_eval {:.3} checksum {}",
        rows.len(),
        evals,
        eval_seconds * 1000.0,
        evals as f64 / eval_seconds,
        eval_seconds * 1_000_000.0 / evals as f64,
        checksum
    );

    let mut update_pairs = 0usize;
    let mut update_checksum = 0i64;
    let started = Instant::now();
    for (_, position) in &rows {
        assert!(evaluator.reset(position));
        for mv in position.legal_moves() {
            let (child, undo) = position.play_tracked(mv);
            for _ in 0..update_repeats {
                black_box(&mut evaluator).push(black_box(&child), undo);
                update_checksum = update_checksum.wrapping_add(i64::from(
                    black_box(&evaluator)
                        .evaluate(black_box(&child))
                        .expect("child NNUE evaluation"),
                ));
                black_box(&mut evaluator).pop();
                update_checksum = update_checksum.wrapping_add(i64::from(
                    black_box(&evaluator)
                        .evaluate(black_box(position))
                        .expect("restored NNUE evaluation"),
                ));
                update_pairs += 1;
            }
        }
    }
    let update_seconds = started.elapsed().as_secs_f64();
    let estimated_update_seconds =
        (update_seconds - 2.0 * update_pairs as f64 * eval_seconds / evals as f64).max(0.0);
    println!(
        "nnue_update positions {} push_pop_pairs {} paired_eval_count {} total_time_ms {:.3} pairs_per_sec {:.1} estimated_push_pop_us {:.3} checksum {}",
        rows.len(),
        update_pairs,
        update_pairs * 2,
        update_seconds * 1000.0,
        update_pairs as f64 / update_seconds,
        estimated_update_seconds * 1_000_000.0 / update_pairs as f64,
        update_checksum
    );
}
