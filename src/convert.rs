use crate::data::{Board, Piece, PieceLocation, Placement, Rotation};

impl From<tbp::Piece> for Piece {
    fn from(p: tbp::Piece) -> Self {
        match p {
            tbp::Piece::I => Piece::I,
            tbp::Piece::O => Piece::O,
            tbp::Piece::T => Piece::T,
            tbp::Piece::L => Piece::L,
            tbp::Piece::J => Piece::J,
            tbp::Piece::S => Piece::S,
            tbp::Piece::Z => Piece::Z,
        }
    }
}

impl From<Piece> for tbp::Piece {
    fn from(p: Piece) -> Self {
        match p {
            Piece::I => tbp::Piece::I,
            Piece::O => tbp::Piece::O,
            Piece::T => tbp::Piece::T,
            Piece::L => tbp::Piece::L,
            Piece::J => tbp::Piece::J,
            Piece::S => tbp::Piece::S,
            Piece::Z => tbp::Piece::Z,
        }
    }
}

impl From<tbp::Orientation> for Rotation {
    fn from(r: tbp::Orientation) -> Self {
        match r {
            tbp::Orientation::North => Rotation::North,
            tbp::Orientation::East => Rotation::East,
            tbp::Orientation::South => Rotation::South,
            tbp::Orientation::West => Rotation::West,
        }
    }
}

impl From<Rotation> for tbp::Orientation {
    fn from(r: Rotation) -> Self {
        match r {
            Rotation::North => tbp::Orientation::North,
            Rotation::East => tbp::Orientation::East,
            Rotation::South => tbp::Orientation::South,
            Rotation::West => tbp::Orientation::West,
        }
    }
}

impl From<tbp::PieceLocation> for PieceLocation {
    fn from(l: tbp::PieceLocation) -> Self {
        PieceLocation {
            piece: l.kind.into(),
            rotation: l.orientation.into(),
            x: l.x as i8,
            y: l.y as i8,
        }
    }
}

impl From<PieceLocation> for tbp::PieceLocation {
    fn from(l: PieceLocation) -> Self {
        tbp::PieceLocation {
            kind: l.piece.into(),
            orientation: l.rotation.into(),
            x: l.x as i32,
            y: l.y as i32,
        }
    }
}

impl From<tbp::Move> for Placement {
    fn from(mv: tbp::Move) -> Self {
        Placement {
            location: mv.location.into(),
            spin: mv.spin,
        }
    }
}

impl From<Placement> for tbp::Move {
    fn from(mv: Placement) -> Self {
        tbp::Move {
            location: mv.location.into(),
            spin: mv.spin,
        }
    }
}

impl From<[[Option<char>; 10]; 40]> for Board {
    fn from(_: [[Option<char>; 10]; 40]) -> Self {
        todo!()
    }
}
