use enum_map::Enum;
use enumset::{EnumSet, EnumSetType};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "Vec<[Option<char>; 10]>")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PieceLocation {
    #[serde(rename = "type")]
    pub piece: Piece,
    #[serde(rename = "orientation")]
    pub rotation: Rotation,
    pub x: i8,
    pub y: i8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Placement {
    pub location: PieceLocation,
    pub spin: Spin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlacementInfo {
    pub placement: Placement,
    pub lines_cleared: u32,
    pub combo: u32,
    pub back_to_back: bool,
    pub perfect_clear: bool,
}

#[allow(clippy::derive_hash_xor_eq)]
#[derive(EnumSetType, Enum, Debug, Hash, Serialize, Deserialize)]
pub enum Piece {
    I,
    O,
    T,
    L,
    J,
    S,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotation {
    North,
    West,
    South,
    East,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spin {
    None,
    Mini,
    Full,
}

impl Piece {
    pub const fn cells(self) -> [(i8, i8); 4] {
        match self {
            Piece::I => [(-1, 0), (0, 0), (1, 0), (2, 0)],
            Piece::O => [(0, 0), (1, 0), (0, 1), (1, 1)],
            Piece::T => [(-1, 0), (0, 0), (1, 0), (0, 1)],
            Piece::L => [(-1, 0), (0, 0), (1, 0), (1, 1)],
            Piece::J => [(-1, 0), (0, 0), (1, 0), (-1, 1)],
            Piece::S => [(-1, 0), (0, 0), (0, 1), (1, 1)],
            Piece::Z => [(-1, 1), (0, 1), (0, 0), (1, 0)],
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

    pub const fn rotate_cells(self, cells: [(i8, i8); 4]) -> [(i8, i8); 4] {
        [
            self.rotate_cell(cells[0]),
            self.rotate_cell(cells[1]),
            self.rotate_cell(cells[2]),
            self.rotate_cell(cells[3]),
        ]
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

macro_rules! lutify {
    (($e:expr) for $v:ident in [$($val:expr),*]) => {
        [
            $(
                {
                    let $v = $val;
                    $e
                }
            ),*
        ]
    };
}

macro_rules! piece_lut {
    ($v:ident => $e:expr) => {
        lutify!(($e) for $v in [Piece::I, Piece::O, Piece::T, Piece::L, Piece::J, Piece::S, Piece::Z])
    };
}

macro_rules! rotation_lut {
    ($v:ident => $e:expr) => {
        lutify!(($e) for $v in [Rotation::North, Rotation::West, Rotation::South, Rotation::East])
    };
}

impl PieceLocation {
    pub const fn cells(&self) -> [(i8, i8); 4] {
        const LUT: [[[(i8, i8); 4]; 4]; 7] =
            piece_lut!(piece => rotation_lut!(rotation => rotation.rotate_cells(piece.cells())));
        self.translate_cells(LUT[self.piece as usize][self.rotation as usize])
    }

    const fn translate(&self, (x, y): (i8, i8)) -> (i8, i8) {
        (x + self.x, y + self.y)
    }

    const fn translate_cells(&self, cells: [(i8, i8); 4]) -> [(i8, i8); 4] {
        [
            self.translate(cells[0]),
            self.translate(cells[1]),
            self.translate(cells[2]),
            self.translate(cells[3]),
        ]
    }

    pub fn obstructed(&self, board: &Board) -> bool {
        self.cells().iter().any(|&cell| board.occupied(cell))
    }

    pub fn drop_distance(&self, board: &Board) -> i8 {
        self.cells()
            .iter()
            .map(|&(x, y)| board.distance_to_ground(x, y))
            .min()
            .unwrap()
    }

    pub fn above_stack(&self, board: &Board) -> bool {
        self.cells()
            .iter()
            .all(|&(x, y)| y >= 64 - board.cols[x as usize].leading_zeros() as i8)
    }

    pub fn canonical_form(&self) -> PieceLocation {
        match self.piece {
            Piece::T | Piece::J | Piece::L => *self,
            Piece::O => match self.rotation {
                Rotation::North => *self,
                Rotation::East => PieceLocation {
                    rotation: Rotation::North,
                    y: self.y - 1,
                    ..*self
                },
                Rotation::South => PieceLocation {
                    rotation: Rotation::North,
                    x: self.x - 1,
                    y: self.y - 1,
                    ..*self
                },
                Rotation::West => PieceLocation {
                    rotation: Rotation::North,
                    x: self.x - 1,
                    ..*self
                },
            },
            Piece::S | Piece::Z => match self.rotation {
                Rotation::North | Rotation::East => *self,
                Rotation::South => PieceLocation {
                    rotation: Rotation::North,
                    y: self.y - 1,
                    ..*self
                },
                Rotation::West => PieceLocation {
                    rotation: Rotation::East,
                    x: self.x - 1,
                    ..*self
                },
            },
            Piece::I => match self.rotation {
                Rotation::North | Rotation::East => *self,
                Rotation::South => PieceLocation {
                    rotation: Rotation::North,
                    x: self.x - 1,
                    ..*self
                },
                Rotation::West => PieceLocation {
                    rotation: Rotation::East,
                    y: self.y + 1,
                    ..*self
                },
            },
        }
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
        debug_assert!((0..10).contains(&x));
        debug_assert!((0..40).contains(&y));
        if y == 0 {
            return 0;
        }
        (!self.cols[x as usize] << (64 - y)).leading_ones() as i8
    }

    pub fn place(&mut self, piece: PieceLocation) {
        for &(x, y) in &piece.cells() {
            debug_assert!((0..10).contains(&x));
            debug_assert!((0..40).contains(&y));
            self.cols[x as usize] |= 1 << y;
        }
    }

    pub fn line_clears(&self) -> u64 {
        self.cols.iter().fold(!0, |a, b| a & b)
    }

    pub fn remove_lines(&mut self, lines: u64) {
        for c in &mut self.cols {
            clear_lines(c, lines);
        }
    }
}

impl GameState {
    pub fn advance(&mut self, next: Piece, placement: Placement) -> PlacementInfo {
        self.bag.remove(next);
        if self.bag.is_empty() {
            self.bag = EnumSet::all();
        }
        if placement.location.piece != next {
            self.reserve = next;
        }
        self.board.place(placement.location);
        let cleared_mask = self.board.line_clears();
        let mut back_to_back = false;
        if cleared_mask != 0 {
            self.board.remove_lines(cleared_mask);
            let hard = cleared_mask.count_ones() == 4 || !matches!(placement.spin, Spin::None);
            back_to_back = hard && self.back_to_back;
            self.back_to_back = hard;
        } else {
            self.combo = 0;
        }
        PlacementInfo {
            placement,
            lines_cleared: cleared_mask.count_ones(),
            combo: self.combo as u32,
            back_to_back,
            perfect_clear: self.board.cols.iter().all(|&c| c == 0),
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "bmi2"))]
fn clear_lines(col: &mut u64, lines: u64) {
    *col = unsafe {
        // SAFETY: #[cfg()] guard ensures that this instruction exists at compile time
        std::arch::x86_64::_pext_u64(*col, !lines)
    };
}

#[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
fn clear_lines(col: &mut u64, mut lines: u64) {
    while lines != 0 {
        let i = lines.trailing_zeros();
        let mask = (1 << i) - 1;
        *col = *col & mask | *col >> 1 & !mask;
        lines &= !(1 << i);
        lines >>= 1;
    }
}
