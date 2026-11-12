use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn opposite(self) -> Side {
        if self == Side::White {
            Side::Black
        } else {
            Side::White
        }
    }
    pub fn sign(self) -> i32 {
        if self == Side::White {
            1
        } else {
            -1
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Piece {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    pub promotion: Option<Piece>,
    /// Origin square of the rook for castling; also disambiguates Chess960 castles.
    pub castle_rook_from: Option<u8>,
}

impl Move {
    pub fn uci(self) -> String {
        let mut text = format!("{}{}", square_name(self.from), square_name(self.to));
        if let Some(piece) = self.promotion {
            text.push(piece.letter().to_ascii_lowercase());
        }
        text
    }

    pub fn uci_chess960(self) -> String {
        let to = self.castle_rook_from.unwrap_or(self.to);
        let mut text = format!("{}{}", square_name(self.from), square_name(to));
        if let Some(piece) = self.promotion {
            text.push(piece.letter().to_ascii_lowercase());
        }
        text
    }
}

#[derive(Clone)]
pub struct Position {
    board: [Option<(Piece, Side)>; 64],
    piece_masks: [[u64; 6]; 2],
    occupied: [u64; 2],
    pub side: Side,
    castle: u8,                    // white king/queen side, then black king/queen side
    castle_rooks: [Option<u8>; 4], // same order as the rights bits
    en_passant: Option<u8>,
    halfmove: u16,
    fullmove: u16,
}

#[derive(Clone, Copy)]
pub(crate) struct Undo {
    mv: Move,
    moved: Option<(Piece, Side)>,
    captured: Option<(u8, (Piece, Side))>,
    rook: Option<(u8, u8)>,
    castle: u8,
    castle_rooks: [Option<u8>; 4],
    en_passant: Option<u8>,
    halfmove: u16,
    fullmove: u16,
    side: Side,
}

impl Position {
    fn king_square(&self, side: Side) -> Option<u8> {
        let kings = self.piece_masks[side_index(side)][piece_index(Piece::King)];
        (kings != 0).then(|| kings.trailing_zeros() as u8)
    }

    pub fn uci_move(&self, mv: Move, chess960: bool) -> String {
        if chess960 {
            if let Some(rook_from) = mv.castle_rook_from {
                return format!("{}{}", square_name(mv.from), square_name(rook_from));
            }
        }
        if chess960 {
            mv.uci_chess960()
        } else {
            mv.uci()
        }
    }

    pub fn start() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap()
    }

    pub fn from_fen(fen: &str) -> Option<Self> {
        let fields: Vec<_> = fen.split_whitespace().collect();
        if fields.len() != 6 {
            return None;
        }
        let mut board = [None; 64];
        let ranks: Vec<_> = fields[0].split('/').collect();
        if ranks.len() != 8 {
            return None;
        }
        for (fen_rank, row) in ranks.iter().enumerate() {
            let mut file = 0usize;
            for c in row.chars() {
                if let Some(empty) = c.to_digit(10) {
                    file += empty as usize;
                    continue;
                }
                if file >= 8 {
                    return None;
                }
                let side = if c.is_ascii_uppercase() {
                    Side::White
                } else {
                    Side::Black
                };
                let piece = Piece::from_letter(c)?;
                board[(7 - fen_rank) * 8 + file] = Some((piece, side));
                file += 1;
            }
            if file != 8 {
                return None;
            }
        }
        let side = match fields[1] {
            "w" => Side::White,
            "b" => Side::Black,
            _ => return None,
        };
        let mut castle = 0;
        let mut castle_rooks = [None; 4];
        if fields[2] != "-" {
            for c in fields[2].chars() {
                let (side, rook_file) = match c {
                    'K' => (Side::White, 7),
                    'Q' => (Side::White, 0),
                    'k' => (Side::Black, 7),
                    'q' => (Side::Black, 0),
                    'A'..='H' => (Side::White, c as u8 - b'A'),
                    'a'..='h' => (Side::Black, c as u8 - b'a'),
                    _ => return None,
                };
                let home_rank = if side == Side::White { 0 } else { 7 };
                let rook_sq = home_rank * 8 + rook_file;
                let king_sq = self::king_home_square(&board, side)?;
                let kingside = rook_file > king_sq % 8;
                if rook_file == king_sq % 8 || board[rook_sq as usize] != Some((Piece::Rook, side))
                {
                    return None;
                }
                let right_index = match (side, kingside) {
                    (Side::White, true) => 0,
                    (Side::White, false) => 1,
                    (Side::Black, true) => 2,
                    (Side::Black, false) => 3,
                };
                let bit = 1 << right_index;
                if castle & bit != 0 {
                    return None;
                }
                castle |= bit;
                castle_rooks[right_index] = Some(rook_sq);
            }
        }
        let en_passant = if fields[3] == "-" {
            None
        } else {
            let square = parse_square(fields[3])?;
            let expected_rank = if side == Side::White { 5 } else { 2 };
            if square / 8 != expected_rank {
                return None;
            }
            Some(square)
        };
        for side in [Side::White, Side::Black] {
            let kings = board
                .iter()
                .filter(|piece| **piece == Some((Piece::King, side)))
                .count();
            if kings != 1 {
                return None;
            }
        }
        let mut position = Self {
            board,
            piece_masks: [[0; 6]; 2],
            occupied: [0; 2],
            side,
            castle,
            castle_rooks,
            en_passant,
            halfmove: fields[4].parse().ok()?,
            fullmove: fields[5].parse().ok()?,
        };
        position.rebuild_bitboards();
        Some(position)
    }

    pub fn to_fen(&self) -> String {
        let mut placement = String::new();
        for rank in (0..8).rev() {
            if rank != 7 {
                placement.push('/');
            }
            let mut empty = 0;
            for file in 0..8 {
                match self.board[rank * 8 + file] {
                    Some((piece, side)) => {
                        if empty > 0 {
                            placement.push(char::from(b'0' + empty));
                            empty = 0;
                        }
                        let letter = piece.letter();
                        placement.push(if side == Side::White {
                            letter
                        } else {
                            letter.to_ascii_lowercase()
                        });
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                placement.push(char::from(b'0' + empty));
            }
        }
        let mut rights = String::new();
        for (index, standard) in ['K', 'Q', 'k', 'q'].into_iter().enumerate() {
            if self.castle & (1 << index) != 0 {
                let rook_sq = self.castle_rooks[index].unwrap_or([7, 0, 63, 56][index]);
                let side = if index < 2 { Side::White } else { Side::Black };
                let king_home_file = self.king_square(side).map(|sq| sq % 8).unwrap_or(4);
                let standard_file = [7, 0, 7, 0][index];
                rights.push(if rook_sq % 8 == standard_file && king_home_file == 4 {
                    standard
                } else if side == Side::White {
                    (b'A' + rook_sq % 8) as char
                } else {
                    (b'a' + rook_sq % 8) as char
                });
            }
        }
        if rights.is_empty() {
            rights.push('-');
        }
        format!(
            "{} {} {} {} {} {}",
            placement,
            if self.side == Side::White { "w" } else { "b" },
            rights,
            self.en_passant
                .map(square_name)
                .unwrap_or_else(|| "-".into()),
            self.halfmove,
            self.fullmove
        )
    }

    pub fn piece_at(&self, sq: u8) -> Option<(Piece, Side)> {
        self.board.get(sq as usize).copied().flatten()
    }

    pub fn is_fifty_move_draw(&self) -> bool {
        self.halfmove >= 100
    }

    pub fn fifty_move_clock(&self) -> u16 {
        self.halfmove
    }

    pub fn piece_count(&self) -> u32 {
        (self.occupied[0] | self.occupied[1]).count_ones()
    }

    pub fn en_passant_square(&self) -> Option<u8> {
        self.en_passant
    }

    pub fn syzygy_bitboards(&self) -> [u64; 8] {
        let white = self.occupied[side_index(Side::White)];
        let black = self.occupied[side_index(Side::Black)];
        [
            white,
            black,
            self.piece_masks[0][piece_index(Piece::King)]
                | self.piece_masks[1][piece_index(Piece::King)],
            self.piece_masks[0][piece_index(Piece::Queen)]
                | self.piece_masks[1][piece_index(Piece::Queen)],
            self.piece_masks[0][piece_index(Piece::Rook)]
                | self.piece_masks[1][piece_index(Piece::Rook)],
            self.piece_masks[0][piece_index(Piece::Bishop)]
                | self.piece_masks[1][piece_index(Piece::Bishop)],
            self.piece_masks[0][piece_index(Piece::Knight)]
                | self.piece_masks[1][piece_index(Piece::Knight)],
            self.piece_masks[0][piece_index(Piece::Pawn)]
                | self.piece_masks[1][piece_index(Piece::Pawn)],
        ]
    }

    pub fn is_insufficient_material(&self) -> bool {
        let mut knights = 0;
        let mut bishops = 0;
        let mut bishop_colors = [false; 2];

        for (square, piece) in self.board.iter().enumerate() {
            match piece.map(|(kind, _)| kind) {
                Some(Piece::Pawn | Piece::Rook | Piece::Queen) => return false,
                Some(Piece::Knight) => knights += 1,
                Some(Piece::Bishop) => {
                    bishops += 1;
                    bishop_colors[(square % 8 + square / 8) % 2] = true;
                }
                _ => {}
            }
        }

        knights + bishops <= 1 || (knights == 0 && (bishop_colors[0] ^ bishop_colors[1]))
    }

    pub fn non_pawn_material(&self, side: Side) -> i32 {
        self.board
            .iter()
            .filter_map(|piece| match piece {
                Some((piece, owner)) if *owner == side => Some(*piece),
                _ => None,
            })
            .filter(|piece| !matches!(piece, Piece::Pawn | Piece::King))
            .map(piece_value)
            .sum()
    }

    pub fn null_move(&self) -> Self {
        let mut next = self.clone();
        next.side = self.side.opposite();
        next.en_passant = None;
        next
    }

    pub fn has_non_pawn_material(&self, side: Side) -> bool {
        self.board.iter().any(|piece| {
            matches!(piece, Some((Piece::Knight | Piece::Bishop | Piece::Rook | Piece::Queen, s)) if *s == side)
        })
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        let mut buffer = [Move::default(); 256];
        let len = self.legal_moves_into(&mut buffer);
        buffer[..len].to_vec()
    }

    pub fn legal_moves_into(&self, output: &mut [Move; 256]) -> usize {
        let mut len = 0;
        self.pseudo_moves_into(output, &mut len);
        let mut legal_len = 0;
        let mut scratch = self.clone();
        for read in 0..len {
            let mv = output[read];
            let undo = scratch.make_move(mv);
            let legal = !scratch.in_check(self.side);
            scratch.unmake_move(undo);
            if legal {
                output[legal_len] = mv;
                legal_len += 1;
            }
        }
        legal_len
    }

    pub fn in_check(&self, side: Side) -> bool {
        let king = self.piece_masks[side_index(side)][piece_index(Piece::King)];
        (king != 0)
            .then(|| king.trailing_zeros() as u8)
            .map(|sq| self.is_attacked(sq, side.opposite()))
            .unwrap_or(true)
    }

    fn rebuild_bitboards(&mut self) {
        self.piece_masks = [[0; 6]; 2];
        self.occupied = [0; 2];
        for (sq, item) in self.board.iter().enumerate() {
            if let Some((piece, side)) = item {
                let bit = 1u64 << sq;
                let side = side_index(*side);
                self.piece_masks[side][piece_index(*piece)] |= bit;
                self.occupied[side] |= bit;
            }
        }
    }

    fn set_square(&mut self, sq: u8, piece: Option<(Piece, Side)>) {
        let bit = 1u64 << sq;
        if let Some((old, side)) = self.board[sq as usize] {
            let side = side_index(side);
            self.piece_masks[side][piece_index(old)] &= !bit;
            self.occupied[side] &= !bit;
        }
        self.board[sq as usize] = piece;
        if let Some((new, side)) = piece {
            let side = side_index(side);
            self.piece_masks[side][piece_index(new)] |= bit;
            self.occupied[side] |= bit;
        }
    }

    pub(crate) fn make_move(&mut self, mv: Move) -> Undo {
        let moving = self.board[mv.from as usize];
        let is_pawn = moving.is_some_and(|p| p.0 == Piece::Pawn);
        let is_castle = mv.castle_rook_from.is_some();
        let ep_capture =
            is_pawn && self.en_passant == Some(mv.to) && self.board[mv.to as usize].is_none();
        let captured_sq = if ep_capture {
            Some(if self.side == Side::White {
                mv.to - 8
            } else {
                mv.to + 8
            })
        } else if !is_castle && self.board[mv.to as usize].is_some() {
            Some(mv.to)
        } else {
            None
        };
        let captured = captured_sq.and_then(|sq| self.board[sq as usize].map(|piece| (sq, piece)));
        let undo = Undo {
            mv,
            moved: moving,
            captured,
            rook: None,
            castle: self.castle,
            castle_rooks: self.castle_rooks,
            en_passant: self.en_passant,
            halfmove: self.halfmove,
            fullmove: self.fullmove,
            side: self.side,
        };
        self.en_passant = None;
        self.set_square(mv.from, None);
        if let Some(sq) = captured_sq {
            self.set_square(sq, None);
        }
        let rook_move = mv.castle_rook_from.map(|rook_from| {
            let rank = mv.from / 8;
            let rook_to = rank * 8 + if rook_from % 8 > mv.from % 8 { 5 } else { 3 };
            (rook_from, rook_to)
        });
        if let Some((rook_from, _)) = rook_move {
            self.set_square(rook_from, None);
        }
        if let Some((piece, side)) = moving {
            self.set_square(mv.to, Some((mv.promotion.unwrap_or(piece), side)));
            if piece == Piece::King {
                self.castle &= if side == Side::White { !3 } else { !12 };
                let first = side_index(side) * 2;
                self.castle_rooks[first] = None;
                self.castle_rooks[first + 1] = None;
            }
            if piece == Piece::Rook {
                self.revoke_rook_right(mv.from);
            }
            if is_pawn && mv.from.abs_diff(mv.to) == 16 {
                self.en_passant = Some((mv.from + mv.to) / 2);
            }
        }
        if let Some((rook_from, rook_to)) = rook_move {
            self.set_square(rook_to, Some((Piece::Rook, undo.side)));
            self.castle_rooks
                .iter_mut()
                .enumerate()
                .for_each(|(i, sq)| {
                    if *sq == Some(rook_from) {
                        self.castle &= !(1 << i);
                        *sq = None;
                    }
                });
        }
        if captured.is_some() {
            self.revoke_rook_right(mv.to);
        }
        self.halfmove = if is_pawn || captured.is_some() {
            0
        } else {
            self.halfmove.saturating_add(1)
        };
        if self.side == Side::Black {
            self.fullmove = self.fullmove.saturating_add(1);
        }
        self.side = self.side.opposite();
        Undo {
            rook: rook_move,
            ..undo
        }
    }

    pub(crate) fn unmake_move(&mut self, undo: Undo) {
        let mv = undo.mv;
        self.set_square(mv.to, None);
        if let Some((from, to)) = undo.rook {
            self.set_square(to, None);
            self.set_square(from, Some((Piece::Rook, undo.side)));
        }
        self.set_square(mv.from, undo.moved);
        if let Some((sq, piece)) = undo.captured {
            self.set_square(sq, Some(piece));
        }
        self.side = undo.side;
        self.castle = undo.castle;
        self.castle_rooks = undo.castle_rooks;
        self.en_passant = undo.en_passant;
        self.halfmove = undo.halfmove;
        self.fullmove = undo.fullmove;
    }

    pub fn play(&self, mv: Move) -> Self {
        let mut next = self.clone();
        next.make_move(mv);
        next
    }

    fn hash_without_clock(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.board.hash(&mut hasher);
        self.side.hash(&mut hasher);
        self.castle.hash(&mut hasher);
        self.castle_rooks.hash(&mut hasher);
        self.en_passant.hash(&mut hasher);
        hasher.finish()
    }

    pub fn repetition_hash(&self) -> u64 {
        self.hash_without_clock()
    }

    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.board.hash(&mut hasher);
        self.side.hash(&mut hasher);
        self.castle.hash(&mut hasher);
        self.castle_rooks.hash(&mut hasher);
        self.en_passant.hash(&mut hasher);
        self.halfmove.min(100).hash(&mut hasher);
        hasher.finish()
    }

    pub fn evaluate(&self) -> i32 {
        crate::nnue::evaluate(self).unwrap_or_else(|| self.evaluate_classical())
    }

    pub(crate) fn evaluate_classical(&self) -> i32 {
        let mut mg = 0;
        let mut eg = 0;
        let mut phase = 0;
        let mut pawn_files = [[0u8; 8]; 2];
        let mut pawn_ranks = [[0u8; 8]; 2];
        let mut bishops = [0u8; 2];
        let mut kings = [None; 2];
        let side_index = |side| if side == Side::White { 0 } else { 1 };

        for (sq, item) in self.board.iter().enumerate() {
            let Some((piece, side)) = item else { continue };
            let idx = side_index(*side);
            let file = (sq % 8) as i32;
            let raw_rank = (sq / 8) as i32;
            let rank = if *side == Side::White {
                raw_rank
            } else {
                7 - raw_rank
            };
            let center = 7 - ((file - 3).abs() + (raw_rank - 3).abs());
            let base = match piece {
                Piece::Pawn => 100,
                Piece::Knight => 320,
                Piece::Bishop => 335,
                Piece::Rook => 500,
                Piece::Queen => 950,
                Piece::King => 0,
            };
            let (mg_pos, eg_pos, phase_value) = match piece {
                Piece::Pawn => (rank * 7 + center * 3, rank * 12 + center * 2, 0),
                Piece::Knight => (center * 10 - (rank == 0) as i32 * 12, center * 7, 1),
                Piece::Bishop => (center * 5 + rank * 2, center * 6, 1),
                Piece::Rook => (rank * 2, rank * 5, 2),
                Piece::Queen => (center * 2, center * 4, 4),
                Piece::King => (-center * 9 - rank * 3, center * 10, 0),
            };
            mg += side.sign() * (base + mg_pos);
            eg += side.sign() * (base + eg_pos);
            phase += phase_value;
            if *piece == Piece::Pawn {
                pawn_files[idx][file as usize] += 1;
                pawn_ranks[idx][file as usize] |= 1 << raw_rank;
            }
            if *piece == Piece::Bishop {
                bishops[idx] += 1;
            }
            if *piece == Piece::King {
                kings[idx] = Some(sq as u8);
            }
            let mobility_weight = match piece {
                Piece::Knight | Piece::Bishop => 3,
                Piece::Rook => 2,
                Piece::Queen => 1,
                _ => 0,
            };
            if mobility_weight > 0 {
                let mobility = self.piece_mobility(sq as u8, *piece, *side);
                mg += side.sign() * mobility * mobility_weight;
                eg += side.sign() * mobility * mobility_weight;
            }
        }

        for side in [Side::White, Side::Black] {
            let idx = side_index(side);
            let sign = side.sign();
            if let Some(king_sq) = kings[idx] {
                let king_file = (king_sq % 8) as i32;
                let king_rank = (king_sq / 8) as i32;
                let shield_rank = king_rank + if side == Side::White { 1 } else { -1 };
                let mut shield = 0;
                if (0..8).contains(&shield_rank) {
                    for file in (king_file - 1).max(0)..=(king_file + 1).min(7) {
                        if self.board[(shield_rank * 8 + file) as usize]
                            == Some((Piece::Pawn, side))
                        {
                            shield += 1;
                        }
                    }
                }
                mg += sign * shield * 10;
                if phase > 8 && (3..=4).contains(&king_file) && (king_rank <= 1 || king_rank >= 6) {
                    mg -= sign * 18;
                }
            }
            if bishops[idx] >= 2 {
                mg += sign * 28;
                eg += sign * 38;
            }
            for file in 0..8 {
                let count = pawn_files[idx][file];
                if count > 1 {
                    mg -= sign * 12 * (count as i32 - 1);
                    eg -= sign * 16 * (count as i32 - 1);
                }
                if count > 0
                    && (file == 0 || pawn_files[idx][file - 1] == 0)
                    && (file == 7 || pawn_files[idx][file + 1] == 0)
                {
                    mg -= sign * 13 * count as i32;
                    eg -= sign * 10 * count as i32;
                }
            }

            for (sq, item) in self.board.iter().enumerate() {
                if *item != Some((Piece::Pawn, side)) {
                    continue;
                }
                let file = (sq % 8) as i32;
                let raw_rank = (sq / 8) as i32;
                let rank = if side == Side::White {
                    raw_rank
                } else {
                    7 - raw_rank
                };
                let next_rank = raw_rank + if side == Side::White { 1 } else { -1 };
                let blocked = rank < 7
                    && (0..8).contains(&next_rank)
                    && self.board[(next_rank * 8 + file) as usize].is_some();
                if !blocked
                    && (file > 0 && pawn_files[idx][(file - 1) as usize] > 0
                        || file < 7 && pawn_files[idx][(file + 1) as usize] > 0)
                {
                    mg += sign * 7;
                    eg += sign * 10;
                }
                let ahead_mask = if side == Side::White {
                    !((1u16 << (raw_rank + 1)) - 1) as u8
                } else {
                    ((1u16 << raw_rank) - 1) as u8
                };
                let first_file = (file - 1).max(0) as usize;
                let last_file = (file + 1).min(7) as usize;
                let enemy_idx = side_index(side.opposite());
                let passed =
                    (first_file..=last_file).all(|f| pawn_ranks[enemy_idx][f] & ahead_mask == 0);
                if passed {
                    let bonus = [0, 5, 10, 18, 30, 50, 80, 0][rank as usize];
                    mg += sign * bonus;
                    eg += sign * bonus * 2;
                }
            }

            for (sq, item) in self.board.iter().enumerate() {
                if *item != Some((Piece::Rook, side)) {
                    continue;
                }
                let file = sq % 8;
                let own_pawn =
                    (0..8).any(|r| self.board[r * 8 + file] == Some((Piece::Pawn, side)));
                let enemy_pawn = (0..8)
                    .any(|r| self.board[r * 8 + file] == Some((Piece::Pawn, side.opposite())));
                if !own_pawn {
                    mg += sign * if enemy_pawn { 8 } else { 16 };
                    eg += sign * if enemy_pawn { 10 } else { 20 };
                }
                let relative_rank = if side == Side::White {
                    sq / 8
                } else {
                    7 - sq / 8
                };
                if relative_rank == 6 {
                    mg += sign * 14;
                    eg += sign * 20;
                }
            }
        }

        let phase = phase.min(24);
        let tapered = (mg * phase + eg * (24 - phase)) / 24;
        tapered * self.side.sign()
    }

    pub fn captured_value(&self, mv: Move) -> i32 {
        let target = self.board[mv.to as usize].map(|p| p.0);
        let captured = target.or_else(|| {
            if self.en_passant == Some(mv.to)
                && self.board[mv.from as usize].map(|p| p.0) == Some(Piece::Pawn)
            {
                Some(Piece::Pawn)
            } else {
                None
            }
        });
        captured.map(piece_value).unwrap_or(0)
    }

    pub fn static_exchange_eval(&self, mv: Move) -> i32 {
        let Some((moving_piece, _)) = self.board[mv.from as usize] else {
            return 0;
        };
        let promotion_gain = mv
            .promotion
            .map(|piece| piece_value(piece) - piece_value(Piece::Pawn))
            .unwrap_or(0);
        let captured = self.captured_value(mv);
        if captured == 0 && promotion_gain == 0 {
            return 0;
        }

        let mut gains = [0; 32];
        gains[0] = captured + promotion_gain;
        let mut exchange = self.play(mv);
        let mut occupant = mv.promotion.unwrap_or(moving_piece);
        let mut depth = 0;

        while depth + 1 < gains.len() {
            let Some((from, attacker, promotion)) =
                exchange.least_legal_attacker(mv.to, exchange.side)
            else {
                break;
            };
            depth += 1;
            let promotion_gain = promotion
                .map(|piece| piece_value(piece) - piece_value(Piece::Pawn))
                .unwrap_or(0);
            gains[depth] = piece_value(occupant) + promotion_gain - gains[depth - 1];
            exchange = exchange.play(Move {
                from,
                to: mv.to,
                promotion,
                castle_rook_from: None,
            });
            occupant = promotion.unwrap_or(attacker);
        }

        while depth > 0 {
            gains[depth - 1] = -(-gains[depth - 1]).max(gains[depth]);
            depth -= 1;
        }
        gains[0]
    }

    fn least_legal_attacker(&self, target: u8, side: Side) -> Option<(u8, Piece, Option<Piece>)> {
        if self.board[target as usize].map(|(piece, _)| piece) == Some(Piece::King) {
            return None;
        }
        let mut least: Option<(u8, Piece, Option<Piece>, i32)> = None;
        let mut candidates = self.occupied[side_index(side)];
        while candidates != 0 {
            let from = candidates.trailing_zeros() as u8;
            candidates &= candidates - 1;
            let Some((piece, owner)) = self.board[from as usize] else {
                continue;
            };
            if owner != side || !self.piece_attacks_square(from, target, piece, side) {
                continue;
            }
            let promotion = if piece == Piece::Pawn && (target / 8 == 0 || target / 8 == 7) {
                Some(Piece::Queen)
            } else {
                None
            };
            let mv = Move {
                from,
                to: target,
                promotion,
                castle_rook_from: None,
            };
            if self.play(mv).in_check(side) {
                continue;
            }
            let value = piece_value(piece);
            if least.map(|(_, _, _, best)| value < best).unwrap_or(true) {
                least = Some((from, piece, promotion, value));
            }
        }
        least.map(|(from, piece, promotion, _)| (from, piece, promotion))
    }

    pub fn piece_attacks_square(&self, from: u8, target: u8, piece: Piece, side: Side) -> bool {
        let file = (from % 8) as i8;
        let rank = (from / 8) as i8;
        let target_file = (target % 8) as i8;
        let target_rank = (target / 8) as i8;
        let df = target_file - file;
        let dr = target_rank - rank;
        let diagonal = df.abs() == dr.abs() && df != 0;
        let straight = (df == 0) != (dr == 0);
        let slides = match piece {
            Piece::Bishop => diagonal,
            Piece::Rook => straight,
            Piece::Queen => diagonal || straight,
            _ => false,
        };
        match piece {
            Piece::Pawn => dr == if side == Side::White { 1 } else { -1 } && df.abs() == 1,
            Piece::Knight => (df.abs(), dr.abs()) == (1, 2) || (df.abs(), dr.abs()) == (2, 1),
            Piece::King => df.abs() <= 1 && dr.abs() <= 1 && (df != 0 || dr != 0),
            Piece::Bishop | Piece::Rook | Piece::Queen => {
                if !slides {
                    return false;
                }
                let step_file = df.signum();
                let step_rank = dr.signum();
                let (mut next_file, mut next_rank) = (file + step_file, rank + step_rank);
                while next_file != target_file || next_rank != target_rank {
                    if self.board[index(next_file, next_rank)].is_some() {
                        return false;
                    }
                    next_file += step_file;
                    next_rank += step_rank;
                }
                true
            }
        }
    }

    fn piece_mobility(&self, from: u8, piece: Piece, side: Side) -> i32 {
        let file = (from % 8) as i8;
        let rank = (from / 8) as i8;
        if piece == Piece::Knight {
            return [
                (1, 2),
                (2, 1),
                (2, -1),
                (1, -2),
                (-1, -2),
                (-2, -1),
                (-2, 1),
                (-1, 2),
            ]
            .into_iter()
            .filter(|(df, dr)| inside(file + df, rank + dr))
            .filter(|(df, dr)| {
                self.board[index(file + df, rank + dr)]
                    .map(|(target, target_side)| target_side != side && target != Piece::King)
                    .unwrap_or(true)
            })
            .count() as i32;
        }
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
            _ => return 0,
        };
        let mut count = 0;
        for &(df, dr) in directions {
            let (mut f, mut r) = (file + df, rank + dr);
            while inside(f, r) {
                match self.board[index(f, r)] {
                    Some((target, target_side)) => {
                        if target_side != side && target != Piece::King {
                            count += 1;
                        }
                        break;
                    }
                    None => count += 1,
                }
                f += df;
                r += dr;
            }
        }
        count
    }

    pub fn is_capture(&self, mv: Move) -> bool {
        self.captured_value(mv) != 0
    }

    fn revoke_rook_right(&mut self, sq: u8) {
        for (index, rook) in self.castle_rooks.iter_mut().enumerate() {
            if *rook == Some(sq) {
                self.castle &= !(1 << index);
                *rook = None;
            }
        }
    }

    fn pseudo_moves_into(&self, moves: &mut [Move; 256], len: &mut usize) {
        for from in 0..64u8 {
            let Some((piece, side)) = self.board[from as usize] else {
                continue;
            };
            if side != self.side {
                continue;
            }
            let f = (from % 8) as i8;
            let r = (from / 8) as i8;
            match piece {
                Piece::Pawn => {
                    let step = if side == Side::White { 1 } else { -1 };
                    let start = if side == Side::White { 1 } else { 6 };
                    let one_r = r + step;
                    if inside(f, one_r) && self.board[index(f, one_r)].is_none() {
                        self.push_pawn_move(moves, len, from, index(f, one_r) as u8, one_r);
                        let two_r = r + 2 * step;
                        if r == start && self.board[index(f, two_r)].is_none() {
                            push_move(
                                moves,
                                len,
                                Move {
                                    from,
                                    to: index(f, two_r) as u8,
                                    promotion: None,
                                    castle_rook_from: None,
                                },
                            );
                        }
                    }
                    for df in [-1, 1] {
                        let nf = f + df;
                        let nr = r + step;
                        if !inside(nf, nr) {
                            continue;
                        }
                        let to = index(nf, nr) as u8;
                        if self.board[to as usize]
                            .map(|p| p.1 != side)
                            .unwrap_or(false)
                            && self.board[to as usize]
                                .map(|p| p.0 != Piece::King)
                                .unwrap_or(false)
                            || self.en_passant == Some(to)
                        {
                            self.push_pawn_move(moves, len, from, to, nr);
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
                        self.add_step(moves, len, from, f + df, r + dr, side);
                    }
                }
                Piece::King => {
                    for df in -1..=1 {
                        for dr in -1..=1 {
                            if df != 0 || dr != 0 {
                                self.add_step(moves, len, from, f + df, r + dr, side);
                            }
                        }
                    }
                    self.add_castles(moves, len, side);
                }
                Piece::Bishop | Piece::Rook | Piece::Queen => {
                    let dirs: &[(i8, i8)] = match piece {
                        Piece::Bishop => &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
                        Piece::Rook => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
                        _ => &[
                            (1, 1),
                            (1, -1),
                            (-1, 1),
                            (-1, -1),
                            (1, 0),
                            (-1, 0),
                            (0, 1),
                            (0, -1),
                        ],
                    };
                    for &(df, dr) in dirs {
                        let (mut nf, mut nr) = (f + df, r + dr);
                        while inside(nf, nr) {
                            let to = index(nf, nr) as u8;
                            if let Some((_, target_side)) = self.board[to as usize] {
                                if target_side != side
                                    && self.board[to as usize]
                                        .map(|p| p.0 != Piece::King)
                                        .unwrap_or(false)
                                {
                                    push_move(
                                        moves,
                                        len,
                                        Move {
                                            from,
                                            to,
                                            promotion: None,
                                            castle_rook_from: None,
                                        },
                                    );
                                }
                                break;
                            }
                            push_move(
                                moves,
                                len,
                                Move {
                                    from,
                                    to,
                                    promotion: None,
                                    castle_rook_from: None,
                                },
                            );
                            nf += df;
                            nr += dr;
                        }
                    }
                }
            }
        }
    }

    fn push_pawn_move(
        &self,
        moves: &mut [Move; 256],
        len: &mut usize,
        from: u8,
        to: u8,
        to_rank: i8,
    ) {
        if to_rank == 0 || to_rank == 7 {
            for promotion in [Piece::Queen, Piece::Rook, Piece::Bishop, Piece::Knight] {
                push_move(
                    moves,
                    len,
                    Move {
                        from,
                        to,
                        promotion: Some(promotion),
                        castle_rook_from: None,
                    },
                );
            }
        } else {
            push_move(
                moves,
                len,
                Move {
                    from,
                    to,
                    promotion: None,
                    castle_rook_from: None,
                },
            );
        }
    }

    fn add_step(
        &self,
        moves: &mut [Move; 256],
        len: &mut usize,
        from: u8,
        file: i8,
        rank: i8,
        side: Side,
    ) {
        if !inside(file, rank) {
            return;
        }
        let to = index(file, rank) as u8;
        if self.board[to as usize]
            .map(|p| p.1 != side && p.0 != Piece::King)
            .unwrap_or(true)
        {
            push_move(
                moves,
                len,
                Move {
                    from,
                    to,
                    promotion: None,
                    castle_rook_from: None,
                },
            );
        }
    }

    fn add_castles(&self, moves: &mut [Move; 256], len: &mut usize, side: Side) {
        if self.in_check(side) {
            return;
        }
        let Some(king) = self.king_square(side) else {
            return;
        };
        let rank = if side == Side::White { 0 } else { 7 };
        if king / 8 != rank {
            return;
        }
        for right in side_index(side) * 2..side_index(side) * 2 + 2 {
            if self.castle & (1 << right) == 0 {
                continue;
            }
            let Some(rook_from) = self.castle_rooks[right] else {
                continue;
            };
            if self.board[rook_from as usize] != Some((Piece::Rook, side)) {
                continue;
            }
            let kingside = right % 2 == 0;
            let king_to = rank * 8 + if kingside { 6 } else { 2 };
            let rook_to = rank * 8 + if kingside { 5 } else { 3 };
            let clear = (0..8u8).all(|file| {
                let sq = rank * 8 + file;
                let on_path = file_between(file, king % 8, king_to % 8)
                    || file_between(file, rook_from % 8, rook_to % 8);
                !on_path || sq == king || sq == rook_from || self.board[sq as usize].is_none()
            });
            if !clear {
                continue;
            }
            let enemy = side.opposite();
            let step = (king_to as i8 - king as i8).signum();
            let mut transit = king;
            let mut safe = true;
            loop {
                let mut probe = self.clone();
                probe.set_square(king, None);
                probe.set_square(rook_from, None);
                probe.set_square(transit, Some((Piece::King, side)));
                if probe.is_attacked(transit, enemy) {
                    safe = false;
                    break;
                }
                if transit == king_to {
                    break;
                }
                transit = (transit as i8 + step) as u8;
            }
            if safe {
                push_move(
                    moves,
                    len,
                    Move {
                        from: king,
                        to: king_to,
                        promotion: None,
                        castle_rook_from: Some(rook_from),
                    },
                );
            }
        }
    }

    fn is_attacked(&self, target: u8, by: Side) -> bool {
        let tf = (target % 8) as i8;
        let tr = (target / 8) as i8;
        let mut candidates = self.occupied[side_index(by)];
        while candidates != 0 {
            let from = candidates.trailing_zeros() as u8;
            candidates &= candidates - 1;
            let Some((piece, side)) = self.board[from as usize] else {
                continue;
            };
            if side != by {
                continue;
            }
            let ff = (from % 8) as i8;
            let fr = (from / 8) as i8;
            let df = tf - ff;
            let dr = tr - fr;
            match piece {
                Piece::Pawn => {
                    if dr == if by == Side::White { 1 } else { -1 } && df.abs() == 1 {
                        return true;
                    }
                }
                Piece::Knight => {
                    if (df.abs(), dr.abs()) == (1, 2) || (df.abs(), dr.abs()) == (2, 1) {
                        return true;
                    }
                }
                Piece::King => {
                    if df.abs() <= 1 && dr.abs() <= 1 && (df != 0 || dr != 0) {
                        return true;
                    }
                }
                Piece::Bishop | Piece::Rook | Piece::Queen => {
                    let diagonal = df.abs() == dr.abs() && df != 0;
                    let straight = (df == 0) != (dr == 0);
                    let slides = match piece {
                        Piece::Bishop => diagonal,
                        Piece::Rook => straight,
                        _ => diagonal || straight,
                    };
                    if !slides {
                        continue;
                    }
                    let sf = df.signum();
                    let sr = dr.signum();
                    let (mut f, mut r) = (ff + sf, fr + sr);
                    let mut clear = true;
                    while f != tf || r != tr {
                        if self.board[index(f, r)].is_some() {
                            clear = false;
                            break;
                        }
                        f += sf;
                        r += sr;
                    }
                    if clear {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl Piece {
    fn from_letter(c: char) -> Option<Piece> {
        Some(match c.to_ascii_lowercase() {
            'p' => Piece::Pawn,
            'n' => Piece::Knight,
            'b' => Piece::Bishop,
            'r' => Piece::Rook,
            'q' => Piece::Queen,
            'k' => Piece::King,
            _ => return None,
        })
    }
    fn letter(self) -> char {
        match self {
            Piece::Pawn => 'P',
            Piece::Knight => 'N',
            Piece::Bishop => 'B',
            Piece::Rook => 'R',
            Piece::Queen => 'Q',
            Piece::King => 'K',
        }
    }
}

fn side_index(side: Side) -> usize {
    if side == Side::White {
        0
    } else {
        1
    }
}

fn piece_index(piece: Piece) -> usize {
    match piece {
        Piece::Pawn => 0,
        Piece::Knight => 1,
        Piece::Bishop => 2,
        Piece::Rook => 3,
        Piece::Queen => 4,
        Piece::King => 5,
    }
}

pub fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 100,
        Piece::Knight => 320,
        Piece::Bishop => 335,
        Piece::Rook => 500,
        Piece::Queen => 950,
        Piece::King => 20_000,
    }
}
fn inside(file: i8, rank: i8) -> bool {
    (0..8).contains(&file) && (0..8).contains(&rank)
}
fn push_move(moves: &mut [Move; 256], len: &mut usize, mv: Move) {
    if *len < moves.len() {
        moves[*len] = mv;
        *len += 1;
    }
}
fn index(file: i8, rank: i8) -> usize {
    rank as usize * 8 + file as usize
}
pub fn square_name(sq: u8) -> String {
    format!("{}{}", (b'a' + sq % 8) as char, (b'1' + sq / 8) as char)
}
pub fn parse_square(s: &str) -> Option<u8> {
    let b = s.as_bytes();
    if b.len() != 2 || !(b'a'..=b'h').contains(&b[0]) || !(b'1'..=b'8').contains(&b[1]) {
        return None;
    }
    Some((b[1] - b'1') * 8 + b[0] - b'a')
}

pub fn parse_uci_move(position: &Position, text: &str) -> Option<Move> {
    parse_uci_move_mode(position, text, false)
}

pub fn parse_uci_move_mode(position: &Position, text: &str, chess960: bool) -> Option<Move> {
    let b = text.as_bytes();
    if b.len() != 4 && b.len() != 5 {
        return None;
    }
    let from = parse_square(std::str::from_utf8(&b[0..2]).ok()?)?;
    let to = parse_square(std::str::from_utf8(&b[2..4]).ok()?)?;
    let promotion = if b.len() == 5 {
        Some(match b[4].to_ascii_lowercase() {
            b'q' => Piece::Queen,
            b'r' => Piece::Rook,
            b'b' => Piece::Bishop,
            b'n' => Piece::Knight,
            _ => return None,
        })
    } else {
        None
    };
    position.legal_moves().into_iter().find(|m| {
        m.promotion == promotion
            && (m.from == from && m.to == to
                || chess960 && m.from == from && m.castle_rook_from == Some(to))
    })
}

fn file_between(file: u8, a: u8, b: u8) -> bool {
    file >= a.min(b) && file <= a.max(b)
}

fn king_home_square(board: &[Option<(Piece, Side)>; 64], side: Side) -> Option<u8> {
    let rank = if side == Side::White { 0 } else { 7 };
    (0..8u8)
        .map(|file| rank * 8 + file)
        .find(|&sq| board[sq as usize] == Some((Piece::King, side)))
}

pub fn perft(position: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    position
        .legal_moves()
        .into_iter()
        .map(|mv| perft(&position.play(mv), depth - 1))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{parse_uci_move_mode, perft, Piece, Position, Side};

    #[test]
    fn standard_start_perft_remains_correct() {
        let position = Position::start();
        assert_eq!(perft(&position, 1), 20);
        assert_eq!(perft(&position, 2), 400);
        assert_eq!(perft(&position, 3), 8_902);
        assert_eq!(perft(&position, 4), 197_281);
    }

    #[test]
    fn standard_kiwipete_perft_remains_correct() {
        let position = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .unwrap();
        assert_eq!(perft(&position, 1), 48);
        assert_eq!(perft(&position, 2), 2_039);
        assert_eq!(perft(&position, 3), 97_862);
        assert_eq!(perft(&position, 4), 4_085_603);
    }

    #[test]
    fn standard_special_rule_perft_positions_match() {
        let cases = [
            (
                "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
                [14, 191, 2_812, 43_238],
            ),
            (
                "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
                [6, 264, 9_467, 422_333],
            ),
            (
                "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
                [44, 1_486, 62_379, 2_103_487],
            ),
            (
                "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
                [46, 2_079, 89_890, 3_894_594],
            ),
        ];
        for (fen, counts) in cases {
            let position = Position::from_fen(fen).unwrap();
            for (depth, expected) in (1..=4).zip(counts) {
                assert_eq!(
                    perft(&position, depth),
                    expected,
                    "FEN: {fen}, depth: {depth}"
                );
            }
        }
    }

    #[test]
    fn invalid_fen_en_passant_and_missing_kings_are_rejected() {
        assert!(Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - z9 0 1").is_none());
        assert!(Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - e4 0 1").is_none());
        assert!(Position::from_fen("8/8/8/8/8/8/8/4K3 w - - 0 1").is_none());
    }

    #[test]
    fn chess960_shredder_fen_and_stationary_king_castle_work() {
        let position = Position::from_fen("4k3/8/8/8/8/8/8/R1K4R w HA - 0 1").unwrap();
        assert_eq!(position.to_fen(), "4k3/8/8/8/8/8/8/R1K4R w HA - 0 1");

        let castle = parse_uci_move_mode(&position, "c1a1", true).unwrap();
        assert_eq!(position.uci_move(castle, true), "c1a1");
        let next = position.play(castle);
        assert_eq!(next.piece_at(2), Some((Piece::King, Side::White)));
        assert_eq!(next.piece_at(3), Some((Piece::Rook, Side::White)));
        assert_eq!(next.to_fen(), "4k3/8/8/8/8/8/8/2KR3R b - - 1 1");
    }

    #[test]
    fn chess960_castle_handles_rook_on_king_destination() {
        let position = Position::from_fen("4k3/8/8/8/8/8/8/4K1R1 w G - 0 1").unwrap();
        let castle = parse_uci_move_mode(&position, "e1g1", true).unwrap();
        let next = position.play(castle);
        assert_eq!(next.piece_at(6), Some((Piece::King, Side::White)));
        assert_eq!(next.piece_at(5), Some((Piece::Rook, Side::White)));
        assert_eq!(next.to_fen(), "4k3/8/8/8/8/8/8/5RK1 b - - 1 1");
    }

    #[test]
    fn chess960_castling_rights_are_part_of_the_position_hash() {
        let rook_f = Position::from_fen("4k3/8/8/8/8/8/8/4KRR1 w F - 0 1").unwrap();
        let rook_g = Position::from_fen("4k3/8/8/8/8/8/8/4KRR1 w G - 0 1").unwrap();
        assert_ne!(rook_f.hash(), rook_g.hash());
    }

    #[test]
    fn moving_or_capturing_the_designated_chess960_rook_revokes_castling() {
        let position = Position::from_fen("4k3/8/8/8/8/8/8/4K1RR w G - 0 1").unwrap();
        let castle = parse_uci_move_mode(&position, "e1g1", true).unwrap();
        let castled = position.play(castle);
        assert_eq!(castled.to_fen(), "4k3/8/8/8/8/8/8/5RKR b - - 1 1");

        let move_rook = parse_uci_move_mode(&position, "g1g2", true).unwrap();
        assert!(position.play(move_rook).to_fen().contains(" b - "));

        let capture_position = Position::from_fen("k5r1/8/8/8/8/8/8/4K1RR b G - 0 1").unwrap();
        let capture = parse_uci_move_mode(&capture_position, "g8g1", true).unwrap();
        assert!(capture_position.play(capture).to_fen().contains(" w - "));
    }

    #[test]
    fn chess960_castle_handles_stationary_rook_and_unmake() {
        let mut position = Position::from_fen("4k3/8/8/8/8/8/8/4KR2 w F - 0 1").unwrap();
        let original_fen = position.to_fen();
        let castle = parse_uci_move_mode(&position, "e1f1", true).unwrap();
        assert_eq!(position.uci_move(castle, true), "e1f1");
        let undo = position.make_move(castle);
        assert_eq!(position.piece_at(6), Some((Piece::King, Side::White)));
        assert_eq!(position.piece_at(5), Some((Piece::Rook, Side::White)));
        position.unmake_move(undo);
        assert_eq!(position.to_fen(), original_fen);
    }

    #[test]
    fn chess960_perft_reference_positions_match() {
        let cases = [
            (
                "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
                [21, 528, 12_189],
            ),
            (
                "2nnrbkr/p1qppppp/8/1ppb4/6PP/3PP3/PPP2P2/BQNNRBKR w HEhe - 1 9",
                [21, 807, 18_002],
            ),
            (
                "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
                [20, 479, 10_471],
            ),
            (
                "qbbnnrkr/2pp2pp/p7/1p2pp2/8/P3PP2/1PPP1KPP/QBBNNR1R w hf - 0 9",
                [22, 593, 13_440],
            ),
            (
                "1nbbnrkr/p1p1ppp1/3p4/1p3P1p/3Pq2P/8/PPP1P1P1/QNBBNRKR w HFhf - 0 9",
                [28, 1_120, 31_058],
            ),
        ];
        for (fen, counts) in cases {
            let position = Position::from_fen(fen).unwrap();
            assert_eq!(position.to_fen(), fen);
            for (depth, expected) in (1..=3).zip(counts) {
                assert_eq!(
                    perft(&position, depth),
                    expected,
                    "FEN: {fen}, depth: {depth}"
                );
            }
        }
        let first = Position::from_fen(cases[0].0).unwrap();
        assert_eq!(perft(&first, 4), 326_672);
    }
}
