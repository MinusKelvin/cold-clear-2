use std::ops::Add;

use enum_map::EnumMap;
use enumset::EnumSet;
use ordered_float::OrderedFloat;

use crate::dag::ChildData;
use crate::dag::Dag;
use crate::dag::Evaluation;
use crate::data::*;
use crate::movegen::find_moves;
use crate::profile::ProfileScope;

use super::Mode;
use super::ModeSwitch;

pub struct Freestyle {
    dag: Dag<Eval>,
}

impl Freestyle {
    pub fn new(root: GameState, queue: &[Piece]) -> Self {
        Freestyle {
            dag: Dag::new(root, queue),
        }
    }
}

impl Mode for Freestyle {
    fn advance(&mut self, mv: Placement) -> Option<ModeSwitch> {
        self.dag.advance(mv);
        None
    }

    fn new_piece(&mut self, piece: Piece) {
        self.dag.add_piece(piece);
    }

    fn suggest(&self) -> Vec<Placement> {
        self.dag.suggest()
    }

    fn do_work(&self) {
        if let Some(node) = self.dag.select() {
            let (state, next) = node.state();
            let next_possibilities = next.map(EnumSet::only).unwrap_or(state.bag);

            let mut moves = EnumMap::default();
            for piece in next_possibilities | state.reserve {
                moves[piece] = find_moves(&state.board, piece);
            }

            let mut children: EnumMap<_, Vec<_>> = EnumMap::default();

            for next in next_possibilities {
                let moves = moves[next].iter().chain(if next == state.reserve {
                    [].iter()
                } else {
                    moves[state.reserve].iter()
                });
                for &(mv, sd_distance) in moves {
                    let mut resulting_state = state;
                    let info = resulting_state.advance(next, mv);

                    let (eval, reward) = evaluate(&DEFAULT_WEIGHTS, &state, &info, sd_distance);

                    children[next].push(ChildData {
                        resulting_state,
                        mv,
                        eval,
                        reward,
                    });
                }
            }

            node.expand(children);
        }
    }
}

struct Weights {
    cell_coveredness: f32,
    max_cell_covered_height: u32,
    row_transitions: f32,

    has_back_to_back: f32,
    wasted_t: f32,
    softdrop: f32,

    normal_clears: [f32; 5],
    mini_spin_clears: [f32; 3],
    spin_clears: [f32; 4],
    back_to_back_clear: f32,
    combo_attack: f32,
    perfect_clear: f32,
    perfect_clear_override: bool,
}

static DEFAULT_WEIGHTS: Weights = Weights {
    cell_coveredness: -0.2,
    max_cell_covered_height: 6,
    row_transitions: -0.1,

    has_back_to_back: 0.5,
    wasted_t: -1.5,
    softdrop: -0.1,

    normal_clears: [0.0, -1.5, -1.0, -0.5, 4.0],
    mini_spin_clears: [0.0, -1.5, -1.0],
    spin_clears: [0.0, 1.0, 4.0, 6.0],
    back_to_back_clear: 1.0,
    combo_attack: 1.5,
    perfect_clear: 15.0,
    perfect_clear_override: true,
};

fn evaluate(
    weights: &Weights,
    state: &GameState,
    info: &PlacementInfo,
    softdrop: u32,
) -> (Eval, Reward) {
    let _scope = ProfileScope::new("freestyle eval");

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
    if info.placement.location.piece == Piece::T && !matches!(info.placement.spin, Spin::Full) {
        reward += weights.wasted_t;
    }
    if state.back_to_back {
        eval += weights.has_back_to_back;
    }
    reward += weights.softdrop * softdrop as f32;

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
