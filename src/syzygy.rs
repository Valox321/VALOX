use crate::position::{Piece, Position, Side};
use pyrrhic_rs::{Color, EngineAdapter};

#[derive(Clone)]
pub struct ValoxAdapter;

impl EngineAdapter for ValoxAdapter {
    fn pawn_attacks(color: Color, square: u64) -> u64 {
        let (file, rank) = ((square % 8) as i8, (square / 8) as i8);
        let step = if color == Color::White { 1 } else { -1 };
        [file - 1, file + 1].into_iter().fold(0, |mask, f| {
            let r = rank + step;
            if (0..8).contains(&f) && (0..8).contains(&r) {
                mask | (1u64 << (r * 8 + f))
            } else {
                mask
            }
        })
    }

    fn knight_attacks(square: u64) -> u64 {
        let (f, r) = ((square % 8) as i8, (square / 8) as i8);
        let mut mask = 0;
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
            let (nf, nr) = (f + df, r + dr);
            if (0..8).contains(&nf) && (0..8).contains(&nr) {
                mask |= 1u64 << (nr * 8 + nf);
            }
        }
        mask
    }

    fn bishop_attacks(square: u64, occupied: u64) -> u64 {
        rays(square, occupied, &[(1, 1), (1, -1), (-1, 1), (-1, -1)])
    }
    fn rook_attacks(square: u64, occupied: u64) -> u64 {
        rays(square, occupied, &[(1, 0), (-1, 0), (0, 1), (0, -1)])
    }
    fn queen_attacks(square: u64, occupied: u64) -> u64 {
        Self::bishop_attacks(square, occupied) | Self::rook_attacks(square, occupied)
    }
    fn king_attacks(square: u64) -> u64 {
        let (f, r) = ((square % 8) as i8, (square / 8) as i8);
        let mut mask = 0;
        for df in -1..=1 {
            for dr in -1..=1 {
                let (nf, nr) = (f + df, r + dr);
                if (df != 0 || dr != 0) && (0..8).contains(&nf) && (0..8).contains(&nr) {
                    mask |= 1u64 << (nr * 8 + nf);
                }
            }
        }
        mask
    }
}

fn rays(square: u64, occupied: u64, dirs: &[(i8, i8)]) -> u64 {
    let (file, rank) = ((square % 8) as i8, (square / 8) as i8);
    let mut mask = 0;
    for &(df, dr) in dirs {
        let (mut f, mut r) = (file + df, rank + dr);
        while (0..8).contains(&f) && (0..8).contains(&r) {
            let bit = 1u64 << (r * 8 + f);
            mask |= bit;
            if occupied & bit != 0 {
                break;
            }
            f += df;
            r += dr;
        }
    }
    mask
}

pub fn probe(
    tablebase: &pyrrhic_rs::TableBases<ValoxAdapter>,
    position: &Position,
) -> Option<pyrrhic_rs::WdlProbeResult> {
    // Syzygy tables assume a position without castling rights. Probing with
    // rights still present can return a WDL result that does not apply.
    if position.has_castling_rights() {
        return None;
    }
    if position.piece_count() > 7 || position.fifty_move_clock() >= 80 {
        return None;
    }
    let [white, black, kings, queens, rooks, bishops, knights, pawns] = position.syzygy_bitboards();
    let en_passant = legal_en_passant_square(position)
        .map(u32::from)
        .unwrap_or(0);
    tablebase
        .probe_wdl(
            white,
            black,
            kings,
            queens,
            rooks,
            bishops,
            knights,
            pawns,
            en_passant,
            position.side == Side::White,
        )
        .ok()
}

/// Syzygy expects an en-passant square only when an en-passant capture is
/// actually legal. The position may still carry an ep square after a double
/// pawn push when no enemy pawn can take.
fn legal_en_passant_square(position: &Position) -> Option<u8> {
    let en_passant = position.en_passant_square()?;
    let capture_available = position.legal_moves().iter().any(|mv| {
        mv.to == en_passant
            && position
                .piece_at(mv.from)
                .is_some_and(|(piece, side)| piece == Piece::Pawn && side == position.side)
    });
    capture_available.then_some(en_passant)
}
