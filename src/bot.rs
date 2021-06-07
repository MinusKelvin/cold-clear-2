use ordered_float::NotNan;

use crate::dag::{ChildData, Dag, Evaluation};
use crate::data::{GameState, Piece, Placement, PlacementInfo};
use crate::movegen;
use crate::profile::{ProfileScope, profiling_frame_end};

pub struct Bot {
    dag: Dag<NotNan<f64>>
}

impl Bot {
    pub fn new(root: GameState, queue: impl IntoIterator<Item = Piece>) -> Self {
        Bot {
            dag: Dag::new(root, queue),
        }
    }

    pub fn play(&mut self, mv: Placement) {
        profiling_frame_end();
        self.dag.advance(mv);
    }

    pub fn new_piece(&mut self, piece: Piece) {
        self.dag.add_piece(piece);
    }

    pub fn suggest(&self) -> Vec<Placement> {
        self.dag.suggest()
    }

    pub fn do_work(&self) {
        if let Some(node) = self.dag.select() {
            let (state, next) = node.state();
            let next_possibilities = match next {
                Some(p) => p | state.reserve,
                None => state.bag | state.reserve,
            };

            let mut children = vec![];

            for next in next_possibilities {
                for (mv, sd_distance) in movegen::find_moves(&state.board, next) {
                    let mut resulting_state = state;
                    let info = resulting_state.advance(next, mv);

                    let (eval, reward) = dumb_eval(&state, &info);

                    children.push(ChildData {
                        resulting_state,
                        mv,
                        eval: NotNan::new(eval).unwrap(),
                        reward: NotNan::new(reward).unwrap(),
                    });
                }
            }

            node.expand(children);
        }
    }
}

impl Evaluation for NotNan<f64> {
    type Reward = Self;

    fn average(of: impl Iterator<Item = Option<Self>>) -> Self {
        let mut count = 0;
        let mut sum = NotNan::new(0.0).unwrap();
        for v in of {
            count += 1;
            sum += v.unwrap_or(NotNan::new(-1000.0).unwrap());
        }
        if count == 0 {
            NotNan::new(-1000.0).unwrap()
        } else {
            sum / count as f64
        }
    }
}

fn dumb_eval(state: &GameState, info: &PlacementInfo) -> (f64, f64) {
    let _scope = ProfileScope::new("eval");

    let height = state.board.cols.iter().map(|c| c.leading_zeros()).min().unwrap();
    (height as f64 / 10.0, info.lines_cleared as f64)
}
