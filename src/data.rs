use enum_map::Enum;
use enumset::{EnumSet, EnumSetType};

pub use tbp::Spin;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Board {
    pub cols: [u64; 10],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GameState {
    pub board: Board,
    pub bag: EnumSet<Piece>,
    pub reserve: Piece,
    pub back_to_back: bool,
    pub combo: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PieceLocation {
    pub piece: Piece,
    pub rotation: Rotation,
    pub x: i8,
    pub y: i8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Placement {
    pub location: PieceLocation,
    pub spin: Spin,
}

#[derive(EnumSetType, Enum, Debug, Hash)]
pub enum Piece {
    I,
    O,
    T,
    L,
    J,
    S,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rotation {
    North,
    West,
    South,
    East,
}

impl Piece {
    pub const fn cells(self) -> [(i8, i8); 4] {
        match self {
            Piece::I => [(-1, 0), (0, 0), (1, 0), (2, 0)],
            Piece::O => [(0, 0), (1, 0), (0, 1), (1, 1)],
            Piece::T => [(-1, 0), (0, 0), (1, 0), (0, 1)],
            Piece::L => [(-1, 0), (0, 0), (1, 0), (1, 1)],
            Piece::J => [(-1, 0), (0, 0), (1, 0), (-1, 1)],
            Piece::S => [(-1, 0), (0, 0), (1, 0), (1, 1)],
            Piece::Z => [(-1, 1), (0, 0), (1, 0), (0, 1)],
        }
    }
}

impl Rotation {
    pub const fn rotate_cell(self, (x, y): (i8, i8)) -> (i8, i8) {
        match self {
            Rotation::North => (x, y),
            Rotation::East => (y, -x),
            Rotation::South => (-x, -y),
            Rotation::West => (-y, x),
        }
    }

    pub const fn cw(self) -> Self {
        match self {
            Rotation::North => Rotation::East,
            Rotation::East => Rotation::South,
            Rotation::South => Rotation::West,
            Rotation::West => Rotation::North,
        }
    }

    pub const fn ccw(self) -> Self {
        match self {
            Rotation::North => Rotation::West,
            Rotation::East => Rotation::North,
            Rotation::South => Rotation::East,
            Rotation::West => Rotation::South,
        }
    }

    pub const fn flip(self) -> Self {
        match self {
            Rotation::North => Rotation::South,
            Rotation::East => Rotation::West,
            Rotation::South => Rotation::North,
            Rotation::West => Rotation::East,
        }
    }
}

impl PieceLocation {
    pub const fn cells(&self) -> [(i8, i8); 4] {
        let cells = self.piece.cells();
        [
            self.translate(self.rotation.rotate_cell(cells[0])),
            self.translate(self.rotation.rotate_cell(cells[1])),
            self.translate(self.rotation.rotate_cell(cells[2])),
            self.translate(self.rotation.rotate_cell(cells[3])),
        ]
    }

    const fn translate(&self, (x, y): (i8, i8)) -> (i8, i8) {
        (x + self.x, y + self.y)
    }
}

impl Board {
    pub const fn occupied(&self, (x, y): (i8, i8)) -> bool {
        if x < 0 || x >= 10 || y < 0 || y >= 40 {
            return true;
        }
        self.cols[x as usize] & 1 << y != 0
    }

    pub fn distance_to_ground(&self, x: i8, y: i8) -> i8 {
        debug_assert!(x >= 0 && x < 10);
        debug_assert!(y >= 0 && y < 40);
        if y == 0 {
            return 0;
        }
        (!self.cols[x as usize] << (64 - y)).leading_ones() as i8
    }

    pub fn place(&mut self, piece: PieceLocation) {
        for &(x, y) in &piece.cells() {
            debug_assert!(x >= 0 && x < 10);
            debug_assert!(y >= 0 && y < 40);
            self.cols[x as usize] |= 1 << y;
        }
    }

    pub fn line_clears(&self) -> u64 {
        self.cols.iter().fold(!0, |a, b| a & b)
    }

    pub fn remove_lines(&mut self, lines: u64) {
        for c in &mut self.cols {
            *c = pext(*c, !lines);
        }
    }
}

impl GameState {
    pub fn advance(&mut self, next: Piece, placement: Placement) {
        self.bag.remove(next);
        if self.bag.is_empty() {
            self.bag = EnumSet::all();
        }
        if placement.location.piece != next {
            self.reserve = next;
        }
        self.board.place(placement.location);
        let cleared_mask = self.board.line_clears();
        if cleared_mask != 0 {
            self.board.remove_lines(cleared_mask);
            self.back_to_back =
                cleared_mask.count_ones() == 4 || !matches!(placement.spin, Spin::None);
        } else {
            self.combo = 0;
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "bmi2"))]
fn pext(a: u64, mask: u64) -> u64 {
    unsafe { std::arch::x86_64::_pext_u64(a, mask) }
}

#[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
fn pext(a: u64, mask: u64) -> u64 {
    // FIXME: slow pext polyfill
    let mut result = 0;
    let mut n = 0;
    for i in 0..64 {
        if mask & 1 << i != 0 {
            result |= (a & 1 << i) >> (i - n);
            n += 1;
        }
    }
    result
}
