use std::ops::Add;

use enum_map::EnumMap;
use enumset::EnumSet;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use super::{BotOptions, Mode, ModeSwitch, Statistics};
use crate::dag::{ChildData, Dag, Evaluation};
use crate::data::*;
use crate::movegen::find_moves;

pub struct Freestyle {
    dag: Dag<Eval>,
}

impl Freestyle {
    pub fn new(_options: &BotOptions, root: GameState, queue: &[Piece]) -> Self {
        Freestyle {
            dag: Dag::new(root, queue),
        }
    }
}

impl Mode for Freestyle {
    fn advance(&mut self, _options: &BotOptions, mv: Placement) -> Option<ModeSwitch> {
        puffin::profile_function!();
        self.dag.advance(mv);
        None
    }

    fn new_piece(&mut self, _options: &BotOptions, piece: Piece) {
        puffin::profile_function!();
        self.dag.add_piece(piece);
    }

    fn suggest(&self, _options: &BotOptions) -> Vec<Placement> {
        puffin::profile_function!();
        self.dag.suggest()
    }

    fn do_work(&self, options: &BotOptions) -> Statistics {
        puffin::profile_function!();
        let mut new_stats = Statistics::default();
        new_stats.selections += 1;

        if let Some(node) = self
            .dag
            .select(options.speculate, options.config.freestyle_exploitation)
        {
            let (state, next) = node.state();
            let next_possibilities = next.map(EnumSet::only).unwrap_or(state.bag);

            let mut moves = EnumMap::default();
            {
                puffin::profile_scope!("movegen");
                for piece in next_possibilities | state.reserve {
                    moves[piece] = find_moves(&state.board, piece);
                }
            }

            let mut children: EnumMap<_, Vec<_>> = EnumMap::default();

            {
                puffin::profile_scope!("eval");
                for next in next_possibilities {
                    let moves = moves[next].iter().chain(if next == state.reserve {
                        [].iter()
                    } else {
                        moves[state.reserve].iter()
                    });
                    for &(mv, sd_distance) in moves {
                        let mut state = state;
                        let info = state.advance(next, mv);

                        let (eval, reward) =
                            evaluate(&options.config.freestyle_weights, state, &info, sd_distance);

                        children[next].push(ChildData {
                            resulting_state: state,
                            mv,
                            eval,
                            reward,
                        });
                    }

                    new_stats.nodes += children[next].len() as u64;
                }
            }

            new_stats.expansions += 1;
            node.expand(children);
        }

        new_stats
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Weights {
    pub cell_coveredness: f32,
    pub max_cell_covered_height: u32,
    pub holes: f32,
    pub row_transitions: f32,
    pub height: f32,
    pub height_upper_half: f32,
    pub height_upper_quarter: f32,
    pub tetris_well_depth: f32,
    pub tslot: [f32; 4],

    pub has_back_to_back: f32,
    pub wasted_t: f32,
    pub softdrop: f32,

    pub normal_clears: [f32; 5],
    pub mini_spin_clears: [f32; 3],
    pub spin_clears: [f32; 4],
    pub back_to_back_clear: f32,
    pub combo_attack: f32,
    pub perfect_clear: f32,
    pub perfect_clear_override: bool,
}

fn evaluate(
    weights: &Weights,
    mut state: GameState,
    info: &PlacementInfo,
    softdrop: u32,
) -> (Eval, Reward) {
    let mut eval = 0.0;
    let mut reward = 0.0;

    // line clear rewards
    if info.perfect_clear {
        reward += weights.perfect_clear;
    }
    if !info.perfect_clear || !weights.perfect_clear_override {
        if info.back_to_back {
            reward += weights.back_to_back_clear;
        }
        match info.placement.spin {
            Spin::None => reward += weights.normal_clears[info.lines_cleared as usize],
            Spin::Mini => reward += weights.mini_spin_clears[info.lines_cleared as usize],
            Spin::Full => reward += weights.spin_clears[info.lines_cleared as usize],
        }
        reward += weights.combo_attack * (info.combo.saturating_sub(1) / 2) as f32;
    }

    // checklist
    if info.placement.location.piece == Piece::T
        && (info.lines_cleared < 2 || !matches!(info.placement.spin, Spin::Full))
    {
        reward += weights.wasted_t;
    }
    if state.back_to_back {
        eval += weights.has_back_to_back;
    }
    reward += weights.softdrop * softdrop as f32;

    // cutouts
    let cutout_count = state.bag.contains(Piece::T) as usize
        + (state.reserve == Piece::T) as usize
        + (state.bag.len() <= 3) as usize;
    for _ in 0..cutout_count {
        let location =
            well_known_tslot_left(&state.board).or_else(|| well_known_tslot_right(&state.board));
        let location = match location {
            Some(v) => v,
            None => break,
        };
        let mut board = state.board;
        board.place(location);
        eval += weights.tslot[board.line_clears().count_ones() as usize];
        if board.line_clears().count_ones() > 1 {
            board.remove_lines(board.line_clears());
            state.board = board;
        }
    }

    // holes
    eval += weights.holes
        * state
            .board
            .cols
            .iter()
            .map(|&c| {
                let height = 64 - c.leading_zeros();
                let underneath = (1 << height) - 1;
                let holes = !c & underneath;
                holes.count_ones()
            })
            .sum::<u32>() as f32;

    // cell coveredness
    let mut coveredness = 0;
    for &c in &state.board.cols {
        let height = 64 - c.leading_zeros();
        let underneath = (1 << height) - 1;
        let mut holes = !c & underneath;
        while holes != 0 {
            let y = holes.trailing_zeros();
            coveredness += (height - y).min(weights.max_cell_covered_height);
            holes &= !(1 << y);
        }
    }
    eval += weights.cell_coveredness * coveredness as f32;

    // tetris well depth
    let (tetris_well_column, tetris_well_height) = state
        .board
        .cols
        .iter()
        .enumerate()
        .map(|(i, &c)| (i, 64 - c.leading_zeros()))
        .min_by_key(|&(_, h)| h)
        .unwrap();
    let full_lines_except_well = state
        .board
        .cols
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != tetris_well_column)
        .map(|(_, &c)| c)
        .fold(!0, |a, b| a & b);
    let tetris_well_depth = (full_lines_except_well >> tetris_well_height).trailing_ones();
    eval += tetris_well_depth as f32 * weights.tetris_well_depth;

    // height
    let highest_point = state
        .board
        .cols
        .iter()
        .map(|&c| 64 - c.leading_zeros())
        .max()
        .unwrap();
    eval += weights.height * highest_point as f32;
    if highest_point > 10 {
        eval += weights.height_upper_half * (highest_point - 10) as f32;
    }
    if highest_point > 15 {
        eval += weights.height_upper_quarter * (highest_point - 15) as f32;
    }

    // row transitions
    let mut row_transitions = 0;
    row_transitions += (!0 ^ state.board.cols[0]).count_ones();
    row_transitions += (!0 ^ state.board.cols[9]).count_ones();
    for cs in state.board.cols.windows(2) {
        row_transitions += (cs[0] ^ cs[1]).count_ones();
    }
    eval += row_transitions as f32 * weights.row_transitions;

    (
        Eval { value: eval.into() },
        Reward {
            value: reward.into(),
        },
    )
}

fn well_known_tslot_left(board: &Board) -> Option<PieceLocation> {
    for (x, cols) in board.cols.windows(3).enumerate() {
        let y = 64 - cols[0].leading_zeros();
        if 64 - cols[1].leading_zeros() >= y {
            continue;
        }
        if !board.occupied((x as i8 + 2, y as i8 - 1)) {
            continue;
        }
        if board.occupied((x as i8 + 2, y as i8)) {
            continue;
        }
        if !board.occupied((x as i8 + 2, y as i8 + 1)) {
            continue;
        }
        return Some(PieceLocation {
            piece: Piece::T,
            rotation: Rotation::South,
            x: x as i8 + 1,
            y: y as i8,
        });
    }
    None
}

fn well_known_tslot_right(board: &Board) -> Option<PieceLocation> {
    for (x, cols) in board.cols.windows(3).enumerate() {
        let y = 64 - cols[2].leading_zeros();
        if 64 - cols[1].leading_zeros() >= y {
            continue;
        }
        if !board.occupied((x as i8, y as i8 - 1)) {
            continue;
        }
        if board.occupied((x as i8, y as i8)) {
            continue;
        }
        if !board.occupied((x as i8, y as i8 + 1)) {
            continue;
        }
        return Some(PieceLocation {
            piece: Piece::T,
            rotation: Rotation::South,
            x: x as i8 + 1,
            y: y as i8,
        });
    }
    None
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Eval {
    value: OrderedFloat<f32>,
}

#[derive(Copy, Clone, Debug)]
struct Reward {
    value: OrderedFloat<f32>,
}

impl Evaluation for Eval {
    type Reward = Reward;

    fn average(of: impl Iterator<Item = Option<Self>>) -> Self {
        let mut count = 0;
        let sum: f32 = of
            .map(|v| {
                count += 1;
                v.map(|e| e.value.0).unwrap_or(-1000.0)
            })
            .sum();
        Eval {
            value: (sum / count as f32).into(),
        }
    }
}

impl Add<Reward> for Eval {
    type Output = Self;

    fn add(self, rhs: Reward) -> Eval {
        Eval {
            value: self.value + rhs.value,
        }
    }
}
