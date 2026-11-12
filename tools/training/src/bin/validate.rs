use std::{
    env, fs,
    io::{BufRead, BufReader},
    path::Path,
};
use valox_engine::position::{Piece, Position, Side};

const INPUTS: usize = 768;
const BUCKET_LAYOUT: [usize; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3,
];

struct Net {
    hidden: usize,
    buckets: usize,
    first: Vec<i16>,
    first_bias: Vec<i16>,
    output: Vec<i16>,
    output_bias: i16,
}

impl Net {
    fn load(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if bytes.len() < 197_378 || bytes.len() % 2 != 0 {
            return Err(format!(
                "unexpected quantised net size: {} bytes",
                bytes.len()
            ));
        }
        let values = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>();
        let (hidden, buckets) = [(128usize, 1usize), (256, 1), (512, 1), (1024, 1), (512, 4)]
            .into_iter()
            .find(|(h, bucket_count)| {
                let raw_size = 2 * (INPUTS * bucket_count * h + 3 * h + 1);
                raw_size.div_ceil(64) * 64 == bytes.len()
            })
            .ok_or_else(|| format!("unsupported network dimensions/size: {} bytes", bytes.len()))?;
        let input_count = INPUTS * buckets;
        let expected = input_count * hidden + hidden + 2 * hidden + 1;
        if values.len() < expected {
            return Err("quantised net is truncated".into());
        }
        let mut at = 0;
        let first = values[at..at + input_count * hidden].to_vec();
        at += input_count * hidden;
        let first_bias = values[at..at + hidden].to_vec();
        at += hidden;
        let output = values[at..at + 2 * hidden].to_vec();
        at += 2 * hidden;
        let output_bias = values[at];
        Ok(Self {
            hidden,
            buckets,
            first,
            first_bias,
            output,
            output_bias,
        })
    }

    fn evaluate(&self, position: &Position) -> i32 {
        let mut us = self.first_bias.clone();
        let mut them = self.first_bias.clone();
        let mut our_king = 0u8;
        let mut opp_king = 0u8;
        for square in 0..64u8 {
            if position.piece_at(square) == Some((Piece::King, position.side)) {
                our_king = normalise_square(square, position.side);
            }
            if position.piece_at(square) == Some((Piece::King, position.side.opposite())) {
                opp_king = normalise_square(square, position.side) ^ 56;
            }
        }
        let (our_flip, our_bucket) = if self.buckets == 1 {
            (0, 0)
        } else {
            king_bucket(our_king)
        };
        let (opp_flip, opp_bucket) = if self.buckets == 1 {
            (0, 0)
        } else {
            king_bucket(opp_king)
        };
        for square in 0..64u8 {
            let Some((piece, color)) = position.piece_at(square) else {
                continue;
            };
            let kind = match piece {
                Piece::Pawn => 0,
                Piece::Knight => 1,
                Piece::Bishop => 2,
                Piece::Rook => 3,
                Piece::Queen => 4,
                Piece::King => 5,
            };
            let is_us = color == position.side;
            // Bullet normalises black-to-move boards by flipping ranks before feature mapping.
            let normalised_square = normalise_square(square, position.side);
            let stm =
                our_bucket + (if is_us { 0 } else { 384 }) + kind * 64 + normalised_square as usize;
            let ntm = opp_bucket
                + (if is_us { 384 } else { 0 })
                + kind * 64
                + (normalised_square ^ 56) as usize;
            let stm = stm ^ our_flip;
            let ntm = ntm ^ opp_flip;
            add_feature(&mut us, &self.first, stm, self.hidden);
            add_feature(&mut them, &self.first, ntm, self.hidden);
        }
        let mut sum = 0i64;
        for (acc, weight) in us.iter().chain(&them).zip(&self.output) {
            let x = (*acc as i32).clamp(0, 255) as i64;
            sum += (x * x) * (*weight as i64);
        }
        // Bullet's saved model uses QA=255, QB=64 and eval_scale=400.
        (((sum / 255 + self.output_bias as i64) * 400) / (255 * 64)) as i32
    }
}

fn normalise_square(square: u8, side: Side) -> u8 {
    square ^ if side == Side::Black { 56 } else { 0 }
}

fn king_bucket(square: u8) -> (usize, usize) {
    let rank = square as usize / 8;
    let file = square as usize % 8;
    let flip = if file > 3 { 7 } else { 0 };
    let mirrored_file = file.min(7 - file);
    (flip, INPUTS * BUCKET_LAYOUT[rank * 4 + mirrored_file])
}

fn add_feature(acc: &mut [i16], weights: &[i16], index: usize, hidden: usize) {
    let base = index * hidden;
    for (a, w) in acc.iter_mut().zip(&weights[base..base + hidden]) {
        *a = a.wrapping_add(*w);
    }
}

fn main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let net_path = args
        .next()
        .ok_or("usage: cargo run --release --bin validate -- <quantised.bin> <validation.data>")?;
    let data_path = args.next().ok_or("missing validation.data")?;
    let net = Net::load(Path::new(&net_path))?;
    let reader = BufReader::new(fs::File::open(&data_path).map_err(|e| e.to_string())?);
    let mut n = 0f64;
    let (mut abs_error, mut squared_error) = (0f64, 0f64);
    let (mut correct_result, mut decisive, mut sign_correct) = (0usize, 0usize, 0usize);
    let (mut current_correct, mut current_brier, mut candidate_brier) = (0usize, 0f64, 0f64);
    for (line_no, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let Some((fen, tail)) = line.split_once(" | ") else {
            return Err(format!("bad row {}", line_no + 1));
        };
        let mut fields = tail.split(" | ");
        let teacher: f64 = fields
            .next()
            .ok_or("missing score")?
            .parse()
            .map_err(|e| format!("score row {}: {e}", line_no + 1))?;
        let result: f64 = fields
            .next()
            .ok_or("missing result")?
            .parse()
            .map_err(|e| format!("result row {}: {e}", line_no + 1))?;
        let position = Position::from_fen(fen).ok_or(format!("invalid FEN row {}", line_no + 1))?;
        let predicted = net.evaluate(&position) as f64;
        let teacher_stm = teacher * position.side.sign() as f64;
        let error = predicted - teacher_stm;
        abs_error += error.abs();
        squared_error += error * error;
        let candidate_white = predicted * position.side.sign() as f64;
        let candidate_p = 1.0 / (1.0 + (-candidate_white / 400.0).exp());
        candidate_brier += (candidate_p - result).powi(2);
        if let Some(current) = valox_engine::nnue::evaluate(&position) {
            let current = current as f64;
            let current_white = current * position.side.sign() as f64;
            let current_p = 1.0 / (1.0 + (-current_white / 400.0).exp());
            current_brier += (current_p - result).powi(2);
            if result != 0.5 {
                let expected = if position.side == Side::White {
                    result > 0.5
                } else {
                    result < 0.5
                };
                if (current > 0.0) == expected {
                    current_correct += 1;
                }
            }
        }
        if result != 0.5 {
            decisive += 1;
            let expected = if position.side == Side::White {
                result > 0.5
            } else {
                result < 0.5
            };
            let predicted_win = predicted > 0.0;
            if expected == predicted_win {
                correct_result += 1;
            }
            if predicted_win == (teacher_stm > 0.0) {
                sign_correct += 1;
            }
        }
        n += 1.0;
    }
    println!("positions: {}", n as usize);
    println!("teacher score MAE (cp): {:.1}", abs_error / n);
    println!("teacher score RMSE (cp): {:.1}", (squared_error / n).sqrt());
    println!("candidate WDL Brier score: {:.4}", candidate_brier / n);
    println!(
        "current Valox NNUE WDL Brier score: {:.4}",
        current_brier / n
    );
    if decisive > 0 {
        println!(
            "result-side sign accuracy: {:.1}% ({}/{})",
            100.0 * correct_result as f64 / decisive as f64,
            correct_result,
            decisive
        );
        println!(
            "teacher score sign accuracy: {:.1}% ({}/{})",
            100.0 * sign_correct as f64 / decisive as f64,
            sign_correct,
            decisive
        );
        println!(
            "current Valox result-side accuracy: {:.1}% ({}/{})",
            100.0 * current_correct as f64 / decisive as f64,
            current_correct,
            decisive
        );
    }
    Ok(())
}
