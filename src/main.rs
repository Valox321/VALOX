use pyrrhic_rs::TableBases;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use valox_engine::engine::{transposition_entries_for_mb, SearchHistory, Searcher};
use valox_engine::position::{parse_uci_move_mode, perft, Position, Side};
use valox_engine::syzygy::ValoxAdapter;

type Worker = JoinHandle<Searcher>;

fn finish_worker(worker: Option<(Worker, Arc<AtomicBool>)>, stop: bool) -> Option<Searcher> {
    let (handle, signal) = worker?;
    if stop {
        signal.store(true, Ordering::Relaxed);
    }
    handle.join().ok()
}

fn command_value(words: &[&str], name: &str) -> Option<u64> {
    words
        .iter()
        .position(|w| *w == name)
        .and_then(|i| words.get(i + 1))
        .and_then(|s| s.parse().ok())
}

fn parse_position(words: &[&str], chess960: bool) -> Option<Position> {
    let mut position = match words.get(1).copied()? {
        "startpos" => Position::start(),
        "fen" => {
            let end = words
                .iter()
                .position(|w| *w == "moves")
                .unwrap_or(words.len());
            Position::from_fen(&words.get(2..end)?.join(" "))?
        }
        _ => return None,
    };
    if let Some(i) = words.iter().position(|w| *w == "moves") {
        for token in &words[i + 1..] {
            position = position.play(parse_uci_move_mode(&position, token, chess960)?);
        }
    }
    Some(position)
}

fn run_benchmark(depth: u32) {
    let cases = [
        (
            "start",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        ),
        (
            "open",
            "r1bq1rk1/ppp2ppp/2np1n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 4 8",
        ),
        (
            "tactics",
            "r2q1rk1/ppp2ppp/2npbn2/2b1p3/2B1P3/2NPBN2/PPP2PPP/R2Q1RK1 w - - 0 9",
        ),
        ("endgame", "8/5pk1/6p1/3P4/2P5/5K2/8/8 w - - 0 45"),
    ];
    let mut total_nodes = 0u64;
    let started = std::time::Instant::now();
    for (name, fen) in cases {
        let Some(position) = Position::from_fen(fen) else {
            eprintln!("invalid benchmark FEN: {name}");
            continue;
        };
        let signal = Arc::new(AtomicBool::new(false));
        let mut searcher = Searcher::new(
            HashMap::new(),
            65_536,
            signal,
            Duration::from_secs(3600),
            false,
        );
        let best = searcher.search(&position, depth);
        total_nodes = total_nodes.saturating_add(searcher.nodes());
        println!(
            "{name}: bestmove {} nodes {}",
            best.map(|mv| mv.uci()).unwrap_or_else(|| "0000".into()),
            searcher.nodes()
        );
    }
    let elapsed = started.elapsed().as_millis().max(1);
    println!(
        "bench depth {depth} nodes {total_nodes} time_ms {elapsed} nps {}",
        total_nodes.saturating_mul(1000) / elapsed as u64
    );
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "--bench") {
        let depth = args
            .get(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(10)
            .clamp(1, 32);
        run_benchmark(depth);
        return;
    }
    let stdin = io::stdin();
    let mut position = Position::start();
    let mut table = HashMap::new();
    let mut search_history = SearchHistory::default();
    let mut hash_mb = 16usize;
    let mut threads = 1usize;
    let mut chess960 = false;
    let mut tablebases = None;
    let mut worker: Option<(Worker, Arc<AtomicBool>)> = None;
    for line in stdin.lock().lines().map_while(Result::ok) {
        let words: Vec<_> = line.split_whitespace().collect();
        match words.first().copied().unwrap_or("") {
            "uci" => println!("id name Valox 0.5.0 NNUE\nid author Valox\noption name Hash type spin default 16 min 1 max 1024\noption name Threads type spin default 1 min 1 max 64\noption name UCI_Chess960 type check default false\noption name SyzygyPath type string default <empty>\noption name Clear Hash type button\nuciok"),
            "isready" => println!("readyok"),
            "ucinewgame" => {
                if let Some(s) = finish_worker(worker.take(), true) { table = s.tt; }
                table.clear(); search_history = SearchHistory::default(); position = Position::start();
            }
            "position" => {
                if let Some(s) = finish_worker(worker.take(), true) { search_history = s.history_state(); table = s.tt; }
                if let Some(next) = parse_position(&words, chess960) { position = next; }
            }
            "setoption" => {
                if let Some(s) = finish_worker(worker.take(), true) { search_history = s.history_state(); table = s.tt; }
                let name = words.iter().position(|w| *w == "name").and_then(|i| words.get(i+1)).copied().unwrap_or("");
                if name == "Clear" { table.clear(); search_history = SearchHistory::default(); }
                if name == "Hash" { if let Some(n) = command_value(&words,"value") { hash_mb = (n as usize).clamp(1,1024); table.clear(); } }
                if name == "Threads" { if let Some(n) = command_value(&words,"value") { threads = (n as usize).clamp(1,64); } }
                if name == "UCI_Chess960" {
                    chess960 = words.iter().position(|w| *w == "value")
                        .and_then(|i| words.get(i + 1))
                        .is_some_and(|v| v.eq_ignore_ascii_case("true"));
                }
                if name == "SyzygyPath" {
                    let path = words.iter().position(|w| *w == "value").map(|i| words[i+1..].join(" ")).unwrap_or_default();
                    tablebases = None;
                    #[cfg(windows)]
                    let path = if path.as_bytes().get(1) == Some(&b':') { path[2..].to_string() } else { path };
                    if !path.trim().is_empty() {
                        match TableBases::<ValoxAdapter>::new(path.trim()) {
                            Ok(tb) => { tablebases = Some(Arc::new(tb)); println!("info string Syzygy tables loaded"); }
                            Err(error) => println!("info string Syzygy load failed: {error:?}"),
                        }
                    }
                }
            }
            "go" => {
                if let Some(s) = finish_worker(worker.take(), true) { search_history = s.history_state(); table = s.tt; }
                let depth = command_value(&words,"depth").unwrap_or(64).clamp(1,64) as u32;
                let node_limit = command_value(&words,"nodes").filter(|nodes| *nodes > 0);
                let time_ms = if let Some(ms) = command_value(&words,"movetime") { ms.max(1) }
                    else if position.side == Side::White && command_value(&words,"wtime").is_some() || position.side == Side::Black && command_value(&words,"btime").is_some() {
                        let (time, inc) = if position.side == Side::White { (command_value(&words,"wtime").unwrap_or(1000),command_value(&words,"winc").unwrap_or(0)) } else { (command_value(&words,"btime").unwrap_or(1000),command_value(&words,"binc").unwrap_or(0)) };
                        // Sudden-death controls usually provide no moves-to-go. Budget
                        // against a longer game so Valox keeps time for the endgame.
                        let divisor = command_value(&words,"movestogo").unwrap_or(60).clamp(1,100);
                        time.saturating_div(divisor).saturating_add(inc.saturating_mul(3)/4).clamp(20,time.max(20))
                    } else if node_limit.is_some() || words.contains(&"infinite") || words.contains(&"depth") { 86_400_000 } else { 3000 };
                let signal = Arc::new(AtomicBool::new(false));
                let thread_signal = signal.clone();
                let pos = position.clone();
                let limit = transposition_entries_for_mb(hash_mb);
                let tt = std::mem::take(&mut table);
                let prior_history = std::mem::take(&mut search_history);
                let thread_count = threads;
                let syzygy = tablebases.clone();
                worker = Some((thread::spawn(move || {
                    let mut searcher = Searcher::new_with_history(tt, limit, thread_signal, Duration::from_millis(time_ms), prior_history, true);
                    searcher.set_tablebases(syzygy);
                    searcher.set_node_limit(node_limit);
                    let best = searcher.search_with_threads(&pos, depth, thread_count);
                    println!("bestmove {}", best.map(|m| pos.uci_move(m, chess960)).unwrap_or_else(|| "0000".into()));
                    let _ = io::stdout().flush();
                    searcher
                }), signal));
            }
            "stop" => { if let Some(s) = finish_worker(worker.take(), true) { search_history = s.history_state(); table = s.tt; } }
            "perft" => if let Some(depth) = words.get(1).and_then(|s| s.parse::<u32>().ok()) { println!("nodes {}", perft(&position,depth)); },
            "quit" => { let _ = finish_worker(worker.take(), true); break; }
            _ => {}
        }
        let _ = io::stdout().flush();
    }
}
