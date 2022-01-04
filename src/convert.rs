use crate::data::{Board, Piece, PieceLocation, Placement, Rotation, Spin};

use tbp::data::{
    Move as TbpMove, Orientation as TbpOrientation, Piece as TbpPiece,
    PieceLocation as TbpPieceLocation, Spin as TbpSpin,
};
use tbp::MaybeUnknown;

impl TryFrom<MaybeUnknown<TbpPiece>> for Piece {
    type Error = ConvertError;
    fn try_from(p: MaybeUnknown<TbpPiece>) -> Result<Self, ConvertError> {
        Ok(match p {
            MaybeUnknown::Known(TbpPiece::I) => Piece::I,
            MaybeUnknown::Known(TbpPiece::O) => Piece::O,
            MaybeUnknown::Known(TbpPiece::T) => Piece::T,
            MaybeUnknown::Known(TbpPiece::L) => Piece::L,
            MaybeUnknown::Known(TbpPiece::J) => Piece::J,
            MaybeUnknown::Known(TbpPiece::S) => Piece::S,
            MaybeUnknown::Known(TbpPiece::Z) => Piece::Z,
            _ => return Err(ConvertError::new("piece", p)),
        })
    }
}

impl From<Piece> for MaybeUnknown<TbpPiece> {
    fn from(p: Piece) -> Self {
        MaybeUnknown::Known(match p {
            Piece::I => TbpPiece::I,
            Piece::O => TbpPiece::O,
            Piece::T => TbpPiece::T,
            Piece::L => TbpPiece::L,
            Piece::J => TbpPiece::J,
            Piece::S => TbpPiece::S,
            Piece::Z => TbpPiece::Z,
        })
    }
}

impl TryFrom<MaybeUnknown<TbpOrientation>> for Rotation {
    type Error = ConvertError;
    fn try_from(r: MaybeUnknown<TbpOrientation>) -> Result<Self, ConvertError> {
        Ok(match r {
            MaybeUnknown::Known(TbpOrientation::North) => Rotation::North,
            MaybeUnknown::Known(TbpOrientation::East) => Rotation::East,
            MaybeUnknown::Known(TbpOrientation::South) => Rotation::South,
            MaybeUnknown::Known(TbpOrientation::West) => Rotation::West,
            _ => return Err(ConvertError::new("orientation", r)),
        })
    }
}

impl From<Rotation> for MaybeUnknown<TbpOrientation> {
    fn from(r: Rotation) -> Self {
        MaybeUnknown::Known(match r {
            Rotation::North => TbpOrientation::North,
            Rotation::East => TbpOrientation::East,
            Rotation::South => TbpOrientation::South,
            Rotation::West => TbpOrientation::West,
        })
    }
}

impl TryFrom<MaybeUnknown<TbpSpin>> for Spin {
    type Error = ConvertError;
    fn try_from(s: MaybeUnknown<TbpSpin>) -> Result<Self, ConvertError> {
        Ok(match s {
            MaybeUnknown::Known(TbpSpin::None) => Spin::None,
            MaybeUnknown::Known(TbpSpin::Mini) => Spin::Mini,
            MaybeUnknown::Known(TbpSpin::Full) => Spin::Full,
            _ => return Err(ConvertError::new("spin", s)),
        })
    }
}

impl From<Spin> for MaybeUnknown<TbpSpin> {
    fn from(s: Spin) -> Self {
        MaybeUnknown::Known(match s {
            Spin::Full => TbpSpin::Full,
            Spin::Mini => TbpSpin::Mini,
            Spin::None => TbpSpin::None,
        })
    }
}

impl TryFrom<TbpPieceLocation> for PieceLocation {
    type Error = ConvertError;
    fn try_from(l: TbpPieceLocation) -> Result<Self, ConvertError> {
        Ok(PieceLocation {
            piece: l.kind.try_into()?,
            rotation: l.orientation.try_into()?,
            x: l.x as i8,
            y: l.y as i8,
        })
    }
}

impl From<PieceLocation> for TbpPieceLocation {
    fn from(l: PieceLocation) -> Self {
        TbpPieceLocation::new(l.piece.into(), l.rotation.into(), l.x as i32, l.y as i32)
    }
}

impl TryFrom<TbpMove> for Placement {
    type Error = ConvertError;
    fn try_from(mv: TbpMove) -> Result<Self, ConvertError> {
        Ok(Placement {
            location: mv.location.try_into()?,
            spin: mv.spin.try_into()?,
        })
    }
}

impl From<Placement> for TbpMove {
    fn from(mv: Placement) -> Self {
        TbpMove::new(mv.location.into(), mv.spin.into())
    }
}

impl From<Vec<Vec<Option<char>>>> for Board {
    fn from(board: Vec<Vec<Option<char>>>) -> Self {
        let mut cols = [0; 10];
        #[allow(clippy::needless_range_loop)]
        for y in 0..40 {
            for x in 0..10 {
                if board[y][x].is_some() {
                    cols[x] |= 1 << y;
                }
            }
        }
        Board { cols }
    }
}

#[derive(Debug)]
pub struct ConvertError {
    reason: String,
}

impl ConvertError {
    fn new<K: std::fmt::Debug>(data: &str, value: MaybeUnknown<K>) -> Self {
        ConvertError {
            reason: format!(
                "{:?} into {}",
                match &value {
                    MaybeUnknown::Known(v) => v as &dyn std::fmt::Debug,
                    MaybeUnknown::Unknown(v) => v,
                },
                data
            ),
        }
    }
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "conversion failure of {}", self.reason)
    }
}

impl std::error::Error for ConvertError {}
