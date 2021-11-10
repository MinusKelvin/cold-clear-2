use crate::data::{Board, Piece, PieceLocation, Placement, Rotation};

use tbp::data::{
    Move as TbpMove, Orientation as TbpOrientation, Piece as TbpPiece,
    PieceLocation as TbpPieceLocation,
};

impl From<TbpPiece> for Piece {
    fn from(p: TbpPiece) -> Self {
        match p {
            TbpPiece::I => Piece::I,
            TbpPiece::O => Piece::O,
            TbpPiece::T => Piece::T,
            TbpPiece::L => Piece::L,
            TbpPiece::J => Piece::J,
            TbpPiece::S => Piece::S,
            TbpPiece::Z => Piece::Z,
        }
    }
}

impl From<Piece> for TbpPiece {
    fn from(p: Piece) -> Self {
        match p {
            Piece::I => TbpPiece::I,
            Piece::O => TbpPiece::O,
            Piece::T => TbpPiece::T,
            Piece::L => TbpPiece::L,
            Piece::J => TbpPiece::J,
            Piece::S => TbpPiece::S,
            Piece::Z => TbpPiece::Z,
        }
    }
}

impl From<TbpOrientation> for Rotation {
    fn from(r: TbpOrientation) -> Self {
        match r {
            TbpOrientation::North => Rotation::North,
            TbpOrientation::East => Rotation::East,
            TbpOrientation::South => Rotation::South,
            TbpOrientation::West => Rotation::West,
        }
    }
}

impl From<Rotation> for TbpOrientation {
    fn from(r: Rotation) -> Self {
        match r {
            Rotation::North => TbpOrientation::North,
            Rotation::East => TbpOrientation::East,
            Rotation::South => TbpOrientation::South,
            Rotation::West => TbpOrientation::West,
        }
    }
}

impl From<TbpPieceLocation> for PieceLocation {
    fn from(l: TbpPieceLocation) -> Self {
        PieceLocation {
            piece: l.kind.into(),
            rotation: l.orientation.into(),
            x: l.x as i8,
            y: l.y as i8,
        }
    }
}

impl From<PieceLocation> for TbpPieceLocation {
    fn from(l: PieceLocation) -> Self {
        TbpPieceLocation {
            kind: l.piece.into(),
            orientation: l.rotation.into(),
            x: l.x as i32,
            y: l.y as i32,
        }
    }
}

impl From<TbpMove> for Placement {
    fn from(mv: TbpMove) -> Self {
        Placement {
            location: mv.location.into(),
            spin: mv.spin,
        }
    }
}

impl From<Placement> for TbpMove {
    fn from(mv: Placement) -> Self {
        TbpMove {
            location: mv.location.into(),
            spin: mv.spin,
        }
    }
}

impl From<[[Option<char>; 10]; 40]> for Board {
    fn from(board: [[Option<char>; 10]; 40]) -> Self {
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
