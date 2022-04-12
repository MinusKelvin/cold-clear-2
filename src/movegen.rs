use crate::data::*;

pub struct PlacementMap {
    data: [[[bool; 4]; 25]; 10]
}

impl PlacementMap {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        PlacementMap { data: [[[false; 4]; 25]; 10] }
    }

    #[inline]
    pub fn get(&mut self, placement: &Placement) -> bool {
        self.data[placement.location.x as usize][placement.location.y as usize][placement.location.rotation as usize]
    }

    #[inline]
    pub fn set(&mut self, placement: &Placement, value: bool) {
        self.data[placement.location.x as usize][placement.location.y as usize][placement.location.rotation as usize] = value;
    }

    pub fn clear(&mut self) {
        self.data = [[[false; 4]; 25]; 10];
    }
}

pub fn find_moves(board: &Board, piece: Piece) -> Vec<(Placement, u32)> {
    puffin::profile_function!();
    let mut queue = Vec::with_capacity(1000);
    let mut locks = Vec::with_capacity(64);
    let mut queue_map = PlacementMap::new();
    let mut locks_map = PlacementMap::new();

    let mut init_y: i32 = 19;

    let fast_mode = board.cols.iter().all(|&c| c.leading_zeros() > 64 - 16);
    if fast_mode {
        for &rotation in &[
            Rotation::North,
            Rotation::East,
            Rotation::South,
            Rotation::West,
        ] {
            for x in 0..10 {
                let mut location = PieceLocation {
                    piece,
                    rotation,
                    x,
                    y: 19,
                };
                if location.obstructed(board) {
                    continue;
                }
                let distance = location.drop_distance(board);
                location.y -= distance;
                let mv = Placement {
                    location,
                    spin: Spin::None,
                };

                if location.canonical_form() == location {
                    locks.push((mv, 0));
                }
                
                if let Some(right) = shift(mv.location, board, 1) {
                    if  !right.location.above_stack(board) {
                        if !queue_map.get(&right) {
                            queue.push(right);
                            queue_map.set(&right, true);
                        }
                    }
                }
            
                if let Some(left) = shift(mv.location, board, -1) {
                    if !left.location.above_stack(board) {
                        if !queue_map.get(&left) {
                            queue.push(left);
                            queue_map.set(&left, true);
                        }
                    }
                }

                if piece == Piece::O {
                    continue;
                }
            
                if let Some(cw) = rotate_cw(mv.location, board) {
                    if !cw.location.above_stack(board) {
                        if !queue_map.get(&cw) {
                            queue.push(cw);
                            queue_map.set(&cw, true);
                        }
                    }
                }
            
                if let Some(ccw) = rotate_ccw(mv.location, board) {
                    if !ccw.location.above_stack(board) {
                        if !queue_map.get(&ccw) {
                            queue.push(ccw);
                            queue_map.set(&ccw, true);
                        }
                    }
                }
            }
        }
    } else {
        let mut spawned = PieceLocation {
            piece,
            rotation: Rotation::North,
            x: 4,
            y: 19,
        };
        if spawned.obstructed(board) {
            spawned.y += 1;
            init_y = 20;
            if spawned.obstructed(board) {
                return vec![];
            }
        }
        let spawned = Placement {
            location: spawned,
            spin: Spin::None,
        };

        queue.push(spawned);
        queue_map.set(&spawned, true);
    }

    while let Some(mut placement) = queue.pop() {
        try_expand(&mut queue, &mut queue_map, board, &mut placement, fast_mode);
        try_lock(&mut locks, &mut locks_map, board, &mut placement, init_y);
    }

    locks
}

fn try_expand(
    queue: &mut Vec<Placement>,
    queue_map: &mut PlacementMap,
    board: &Board,
    placement: &mut Placement,
    fast_mode: bool
) {
    let mut drop = placement.clone();
    let drop_distance = drop.location.drop_distance(board);
    drop.location.y -= drop_distance;
    if drop_distance > 0 && !(fast_mode && drop.location.above_stack(board)) && !queue_map.get(&drop)  {
        queue.push(drop);
        queue_map.set(&drop, true);
    }

    if let Some(right) = shift(placement.location, board, 1) {
        if !(fast_mode && right.location.above_stack(board)) && !queue_map.get(&right) {
            queue.push(right);
            queue_map.set(&right, true);
        }
    }

    if let Some(left) = shift(placement.location, board, -1) {
        if !(fast_mode && left.location.above_stack(board)) && !queue_map.get(&left) {
            queue.push(left);
            queue_map.set(&left, true);
        }
    }

    if placement.location.piece == Piece::O {
        return;
    }

    if let Some(cw) = rotate_cw(placement.location, board) {
        if !(fast_mode && cw.location.above_stack(board)) && !queue_map.get(&cw) {
            queue.push(cw);
            queue_map.set(&cw, true);
        }
    }

    if let Some(ccw) = rotate_ccw(placement.location, board) {
        if !(fast_mode && ccw.location.above_stack(board)) && !queue_map.get(&ccw) {
            queue.push(ccw);
            queue_map.set(&ccw, true);
        }
    }
}

fn try_lock(
    locks: &mut Vec<(Placement, u32)>,
    locks_map: &mut PlacementMap,
    board: &Board,
    placement: &mut Placement,
    init_y: i32
) {
    let distance = placement.location.drop_distance(board);
    placement.location.y -= distance;

    if placement.location.y >= 20 {
        return;
    }
    let softdrop = 0;
    if !placement.location.above_stack(board) {
        softdrop = (init_y - placement.location.y as i32).max(0) as u32;
    }

    placement.location = placement.location.canonical_form();
    if !locks_map.get(&placement) {
        locks.push((*placement, softdrop));
        locks_map.set(placement, true);
    }
}

fn shift(mut location: PieceLocation, board: &Board, dx: i8) -> Option<Placement> {
    location.x += dx;
    if location.obstructed(board) {
        return None;
    }
    Some(Placement {
        location,
        spin: Spin::None,
    })
}

fn rotate_cw(from: PieceLocation, board: &Board) -> Option<Placement> {
    if from.piece == Piece::O {
        return None;
    }
    const KICKS: [[[(i8, i8); 5]; 4]; 7] =
        piece_lut!(piece => rotation_lut!(rotation => kicks(piece, rotation, rotation.cw())));
    let unkicked = PieceLocation {
        rotation: from.rotation.cw(),
        ..from
    };
    rotate(
        unkicked,
        board,
        KICKS[from.piece as usize][from.rotation as usize]
            .iter()
            .copied(),
    )
}

fn rotate_ccw(from: PieceLocation, board: &Board) -> Option<Placement> {
    if from.piece == Piece::O {
        return None;
    }
    const KICKS: [[[(i8, i8); 5]; 4]; 7] =
        piece_lut!(piece => rotation_lut!(rotation => kicks(piece, rotation, rotation.ccw())));
    let unkicked = PieceLocation {
        rotation: from.rotation.ccw(),
        ..from
    };
    rotate(
        unkicked,
        board,
        KICKS[from.piece as usize][from.rotation as usize]
            .iter()
            .copied(),
    )
}

const fn offsets(piece: Piece, rotation: Rotation) -> [(i8, i8); 5] {
    match piece {
        Piece::O => match rotation {
            Rotation::North => [(0, 0); 5],
            Rotation::East => [(0, -1); 5],
            Rotation::South => [(-1, -1); 5],
            Rotation::West => [(-1, 0); 5],
        },
        Piece::I => match rotation {
            Rotation::North => [(0, 0), (-1, 0), (2, 0), (-1, 0), (2, 0)],
            Rotation::East => [(-1, 0), (0, 0), (0, 0), (0, 1), (0, -2)],
            Rotation::South => [(-1, 1), (1, 1), (-2, 1), (1, 0), (-2, 0)],
            Rotation::West => [(0, 1), (0, 1), (0, 1), (0, -1), (0, 2)],
        },
        _ => match rotation {
            Rotation::North => [(0, 0); 5],
            Rotation::East => [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
            Rotation::South => [(0, 0); 5],
            Rotation::West => [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
        },
    }
}

const fn kicks(piece: Piece, from: Rotation, to: Rotation) -> [(i8, i8); 5] {
    let mut kicks = [(0, 0); 5];
    let from = offsets(piece, from);
    let to = offsets(piece, to);
    let mut i = 0;
    while i < kicks.len() {
        kicks[i] = (from[i].0 - to[i].0, from[i].1 - to[i].1);
        i += 1;
    }
    kicks
}

fn rotate(
    unkicked: PieceLocation,
    board: &Board,
    kicks: impl Iterator<Item = (i8, i8)>,
) -> Option<Placement> {
    for (i, (dx, dy)) in kicks.enumerate() {
        let target = PieceLocation {
            x: unkicked.x + dx,
            y: unkicked.y + dy,
            ..unkicked
        };
        if target.obstructed(board) {
            continue;
        }

        let spin;
        if target.piece != Piece::T {
            spin = Spin::None;
        } else {
            let corners = [(-1, -1), (1, -1), (-1, 1), (1, 1)]
                .iter()
                .filter(|&&(cx, cy)| board.occupied((cx + target.x, cy + target.y)))
                .count();
            let mini_corners = [(-1, 1), (1, 1)]
                .iter()
                .map(|&c| target.rotation.rotate_cell(c))
                .filter(|&(cx, cy)| board.occupied((cx + target.x, cy + target.y)))
                .count();

            if corners < 3 {
                spin = Spin::None;
            } else if mini_corners == 2 || i == 4 {
                spin = Spin::Full;
            } else {
                spin = Spin::Mini;
            }
        }

        return Some(Placement {
            location: target,
            spin,
        });
    }

    None
}