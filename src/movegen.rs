use std::cmp::Ordering;
use std::collections::BinaryHeap;

use ahash::AHashMap;

use crate::data::*;
use crate::profile::ProfileScope;

pub fn find_moves(board: &Board, piece: Piece) -> Vec<(Placement, u32)> {
    let _scope = ProfileScope::new("movegen");

    let mut queue = BinaryHeap::with_capacity(64);
    let mut values = AHashMap::with_capacity(64);
    let mut locks = AHashMap::with_capacity(64);

    let fast_mode;
    if board.cols.iter().all(|&c| c.leading_zeros() > 64 - 16) {
        fast_mode = true;
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
                queue.push(Intermediate {
                    mv,
                    soft_drops: distance as u32,
                });
                values.insert(mv, distance as u32);
                locks.insert(
                    Placement {
                        location: mv.location.canonical_form(),
                        ..mv
                    },
                    0,
                );
            }
        }
    } else {
        fast_mode = false;

        let mut spawned = PieceLocation {
            piece,
            rotation: Rotation::North,
            x: 4,
            y: 19,
        };
        if spawned.obstructed(board) {
            spawned.y += 1;
            if spawned.obstructed(board) {
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

        let sds = locks
            .entry(Placement {
                location: dropped.location.canonical_form(),
                ..dropped
            })
            .or_insert(expand.soft_drops);
        *sds = expand.soft_drops.min(*sds);

        let mut update_position = |target: Placement, soft_drops: u32| {
            if fast_mode && target.location.above_stack(&board) {
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
        };

        update_position(dropped, expand.soft_drops + drop_dist as u32);

        if let Some(mv) = shift(expand.mv.location, board, -1) {
            update_position(mv, expand.soft_drops);
        }
        if let Some(mv) = shift(expand.mv.location, board, 1) {
            update_position(mv, expand.soft_drops);
        }
        if let Some(mv) = rotate_cw(expand.mv.location, board) {
            update_position(mv, expand.soft_drops);
        }
        if let Some(mv) = rotate_ccw(expand.mv.location, board) {
            update_position(mv, expand.soft_drops);
        }
    }

    locks.into_iter().collect()
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
    let unkicked = PieceLocation {
        rotation: from.rotation.cw(),
        ..from
    };
    rotate(
        unkicked,
        board,
        offsets(from)
            .zip(offsets(unkicked))
            .map(|((x1, y1), (x2, y2))| (x1 - x2, y1 - y2)),
    )
}

fn rotate_ccw(from: PieceLocation, board: &Board) -> Option<Placement> {
    let unkicked = PieceLocation {
        rotation: from.rotation.ccw(),
        ..from
    };
    rotate(
        unkicked,
        board,
        offsets(from)
            .zip(offsets(unkicked))
            .map(|((x1, y1), (x2, y2))| (x1 - x2, y1 - y2)),
    )
}

fn offsets(p: PieceLocation) -> impl Iterator<Item = (i8, i8)> {
    match p.piece {
        Piece::O => match p.rotation {
            Rotation::North => [(0, 0)].iter(),
            Rotation::East => [(0, -1)].iter(),
            Rotation::South => [(-1, -1)].iter(),
            Rotation::West => [(-1, 0)].iter(),
        },
        Piece::I => match p.rotation {
            Rotation::North => [(0, 0), (-1, 0), (2, 0), (-1, 0), (2, 0)].iter(),
            Rotation::East => [(-1, 0), (0, 0), (0, 0), (0, 1), (0, -2)].iter(),
            Rotation::South => [(-1, 1), (1, 1), (-2, 1), (1, 0), (-2, 0)].iter(),
            Rotation::West => [(0, 1), (0, 1), (0, 1), (0, -1), (0, 2)].iter(),
        },
        _ => match p.rotation {
            Rotation::North => [(0, 0); 5].iter(),
            Rotation::East => [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)].iter(),
            Rotation::South => [(0, 0); 5].iter(),
            Rotation::West => [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)].iter(),
        },
    }
    .copied()
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
