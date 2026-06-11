//! Compact move encoding.
//!
//! A move is packed into a `u16` as `from | to << 6 | promotion << 12`:
//!
//! * `from`/`to` are 0..64 square indices (6 bits each),
//! * `promotion` is 0 (none) or a [`Role`] discriminant (3 bits).
//!
//! We deliberately do *not* encode castling/en-passant flags. The consumer
//! always has the full position in hand, so an [`EncodedMove`] is resolved back
//! to a concrete [`Move`] by matching it against the position's legal moves,
//! which side-steps every special-case encoding wrinkle.

use shakmaty::{Move, Position, Role, Square};

/// A move packed into 16 bits. See the module docs for the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EncodedMove(pub u16);

impl EncodedMove {
    pub fn encode(mv: &Move) -> EncodedMove {
        let from = mv.from().expect("normal moves have a from square");
        let to = mv.to();
        let promo = mv.promotion().map_or(0, role_to_bits);
        EncodedMove(u16::from(from) | (u16::from(to) << 6) | ((promo as u16) << 12))
    }

    pub fn from_sq(self) -> Square {
        Square::new((self.0 & 0x3f) as u32)
    }

    pub fn to_sq(self) -> Square {
        Square::new(((self.0 >> 6) & 0x3f) as u32)
    }

    pub fn promotion(self) -> Option<Role> {
        bits_to_role((self.0 >> 12) & 0x7)
    }

    /// Resolve this encoded move against a concrete position, returning the
    /// matching legal [`Move`] (or `None` if it is not legal here).
    pub fn resolve<P: Position>(self, pos: &P) -> Option<Move> {
        let (from, to, promo) = (self.from_sq(), self.to_sq(), self.promotion());
        pos.legal_moves()
            .into_iter()
            .find(|m| m.from() == Some(from) && m.to() == to && m.promotion() == promo)
    }
}

/// All legal moves of `pos`, sorted ascending by their [`EncodedMove`] packing.
///
/// This is the canonical ordering the book's 8-bit move indexes refer to. It
/// depends only on the `EncodedMove` bit layout, never on shakmaty's move
/// generation order, so indexes stay stable across shakmaty upgrades. Chess
/// positions have at most 218 legal moves, so an index always fits in a `u8`.
pub fn canonical_legal<P: Position>(pos: &P) -> Vec<Move> {
    let mut moves: Vec<(u16, Move)> = pos
        .legal_moves()
        .into_iter()
        .map(|m| (EncodedMove::encode(&m).0, m))
        .collect();
    // Encodings are unique among legal moves: castling packs as king→rook
    // squares, which no ordinary legal king move can reproduce.
    moves.sort_unstable_by_key(|(enc, _)| *enc);
    moves.into_iter().map(|(_, m)| m).collect()
}

/// The canonical index of legal move `mv` in `pos` (see [`canonical_legal`]).
pub fn canonical_index<P: Position>(pos: &P, mv: &Move) -> Option<u8> {
    let target = EncodedMove::encode(mv).0;
    let mut encodings: Vec<u16> = pos
        .legal_moves()
        .iter()
        .map(|m| EncodedMove::encode(m).0)
        .collect();
    encodings.sort_unstable();
    encodings
        .binary_search(&target)
        .ok()
        .and_then(|i| u8::try_from(i).ok())
}

fn role_to_bits(role: Role) -> u8 {
    match role {
        Role::Pawn => 1,
        Role::Knight => 2,
        Role::Bishop => 3,
        Role::Rook => 4,
        Role::Queen => 5,
        Role::King => 6,
    }
}

fn bits_to_role(bits: u16) -> Option<Role> {
    match bits {
        1 => Some(Role::Pawn),
        2 => Some(Role::Knight),
        3 => Some(Role::Bishop),
        4 => Some(Role::Rook),
        5 => Some(Role::Queen),
        6 => Some(Role::King),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::Chess;

    #[test]
    fn canonical_index_round_trips() {
        let pos = Chess::default();
        let legal = canonical_legal(&pos);
        assert_eq!(legal.len(), 20);
        for (i, mv) in legal.iter().enumerate() {
            assert_eq!(canonical_index(&pos, mv), Some(i as u8));
        }
        // Sorted by encoding means strictly increasing packed values.
        let encodings: Vec<u16> = legal.iter().map(|m| EncodedMove::encode(m).0).collect();
        assert!(encodings.windows(2).all(|w| w[0] < w[1]));
    }
}
