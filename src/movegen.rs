use std::cmp::Ordering;
use std::collections::BinaryHeap;

use ahash::AHashMap;

use crate::data::*;

pub fn find_moves(board: &Board, piece: Piece) -> Vec<(Placement, u32)> {
    puffin::profile_function!();
    let mut queue = BinaryHeap::new();
    let mut values = AHashMap::new();
    let mut underground_locks = AHashMap::new();
    let mut locks = Vec::with_capacity(64);
    let collision_map = CollisionMaps::new(board, piece);

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
                if collision_map.obstructed(location) {
                    continue;
                }
                let distance = location.drop_distance(board);
                location.y -= distance;
                let mv = Placement {
                    location,
                    spin: Spin::None,
                };

                let mut update_position =
                    update_position(&mut queue, &mut values, fast_mode, board);

                if let Some(mv) = shift(location, &collision_map, -1) {
                    update_position(mv, distance as u32);
                }
                if let Some(mv) = shift(location, &collision_map, 1) {
                    update_position(mv, distance as u32);
                }
                if let Some(mv) = rotate_cw(location, &collision_map, board) {
                    update_position(mv, distance as u32);
                }
                if let Some(mv) = rotate_ccw(location, &collision_map, board) {
                    update_position(mv, distance as u32);
                }

                if location.canonical_form() == location {
                    locks.push((mv, 0));
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
        if collision_map.obstructed(spawned) {
            spawned.y += 1;
            if collision_map.obstructed(spawned) {
                return vec![];
            }
        }
        let spawned = Placement {
            location: spawned,
            spin: Spin::None,
        };
        queue.push(Intermediate {
            soft_drops: 0,
            mv: spawned,
        });
        values.insert(spawned, 0);
    }

    while let Some(expand) = queue.pop() {
        if expand.soft_drops != values.get(&expand.mv).copied().unwrap_or(40) {
            continue;
        }

        let drop_dist = expand.mv.location.drop_distance(board);
        let dropped = Placement {
            location: PieceLocation {
                y: expand.mv.location.y - drop_dist,
                ..expand.mv.location
            },
            spin: if drop_dist == 0 {
                expand.mv.spin
            } else {
                Spin::None
            },
        };

        let sds = underground_locks
            .entry(Placement {
                location: dropped.location.canonical_form(),
                ..dropped
            })
            .or_insert(expand.soft_drops);
        *sds = expand.soft_drops.min(*sds);

        let mut update_position = update_position(&mut queue, &mut values, fast_mode, board);

        update_position(dropped, expand.soft_drops + drop_dist as u32);

        if let Some(mv) = shift(expand.mv.location, &collision_map, -1) {
            update_position(mv, expand.soft_drops);
        }
        if let Some(mv) = shift(expand.mv.location, &collision_map, 1) {
            update_position(mv, expand.soft_drops);
        }
        if let Some(mv) = rotate_cw(expand.mv.location, &collision_map, board) {
            update_position(mv, expand.soft_drops);
        }
        if let Some(mv) = rotate_ccw(expand.mv.location, &collision_map, board) {
            update_position(mv, expand.soft_drops);
        }
    }

    locks.extend(underground_locks.into_iter());
    locks
}

fn update_position<'a>(
    queue: &'a mut BinaryHeap<Intermediate>,
    values: &'a mut AHashMap<Placement, u32>,
    fast_mode: bool,
    board: &'a Board,
) -> impl FnMut(Placement, u32) + 'a {
    move |target: Placement, soft_drops: u32| {
        if fast_mode && target.location.above_stack(board) {
            return;
        }
        let prev_sds = values.entry(target).or_insert(40);
        if soft_drops < *prev_sds {
            *prev_sds = soft_drops;
            queue.push(Intermediate {
                soft_drops,
                mv: target,
            });
        }
    }
}

fn shift(mut location: PieceLocation, collision_map: &CollisionMaps, dx: i8) -> Option<Placement> {
    location.x += dx;
    if collision_map.obstructed(location) {
        return None;
    }
    Some(Placement {
        location,
        spin: Spin::None,
    })
}

fn rotate_cw(
    from: PieceLocation,
    collision_map: &CollisionMaps,
    board: &Board,
) -> Option<Placement> {
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
        collision_map,
        board,
        KICKS[from.piece as usize][from.rotation as usize]
            .iter()
            .copied(),
    )
}

fn rotate_ccw(
    from: PieceLocation,
    collision_map: &CollisionMaps,
    board: &Board,
) -> Option<Placement> {
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
        collision_map,
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
    collision_map: &CollisionMaps,
    board: &Board,
    kicks: impl Iterator<Item = (i8, i8)>,
) -> Option<Placement> {
    for (i, (dx, dy)) in kicks.enumerate() {
        let target = PieceLocation {
            x: unkicked.x + dx,
            y: unkicked.y + dy,
            ..unkicked
        };
        if collision_map.obstructed(target) {
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

#[derive(Clone, Copy, Debug, Eq)]
struct Intermediate {
    mv: Placement,
    soft_drops: u32,
}

impl PartialEq for Intermediate {
    fn eq(&self, other: &Intermediate) -> bool {
        self.soft_drops == other.soft_drops
    }
}

impl Ord for Intermediate {
    fn cmp(&self, other: &Intermediate) -> Ordering {
        self.soft_drops.cmp(&other.soft_drops)
    }
}

impl PartialOrd for Intermediate {
    fn partial_cmp(&self, other: &Intermediate) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct CollisionMaps {
    boards: [[u64; 10]; 4],
}

impl CollisionMaps {
    fn new(board: &Board, piece: Piece) -> Self {
        let mut boards = [[0; 10]; 4];
        for rot in [
            Rotation::North,
            Rotation::West,
            Rotation::South,
            Rotation::East,
        ] {
            for (dx, dy) in rot.rotate_cells(piece.cells()) {
                for x in 0..10 {
                    let c = board.cols.get((x + dx) as usize).copied().unwrap_or(!0);
                    let c = match dy < 0 {
                        true => !(!c << -dy),
                        false => c >> dy,
                    };
                    boards[rot as usize][x as usize] |= c;
                }
            }
        }
        CollisionMaps { boards }
    }

    fn obstructed(&self, piece: PieceLocation) -> bool {
        let v = piece.y < 0
            || self.boards[piece.rotation as usize]
                .get(piece.x as usize)
                .map(|&c| c & 1 << piece.y != 0)
                .unwrap_or(true);
        v
    }
}
