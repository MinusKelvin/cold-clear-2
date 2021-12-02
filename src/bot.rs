use std::collections::VecDeque;
use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::data::{GameState, Piece, Placement};

mod freestyle;

use self::freestyle::Freestyle;

pub struct Bot {
    options: BotOptions,
    current: GameState,
    queue: VecDeque<Piece>,
    mode: ModeEnum,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BotConfig {
    pub freestyle_weights: freestyle::Weights,
    pub freestyle_exploitation: f64,
}

impl Default for BotConfig {
    fn default() -> Self {
        static DEFAULT: Lazy<BotConfig> =
            Lazy::new(|| serde_json::from_str(include_str!("default.json")).unwrap());
        DEFAULT.clone()
    }
}

#[derive(Debug)]
pub struct BotOptions {
    pub speculate: bool,
    pub config: Arc<BotConfig>,
}

#[enum_dispatch]
enum ModeEnum {
    Freestyle,
}

#[enum_dispatch(ModeEnum)]
trait Mode {
    fn advance(&mut self, options: &BotOptions, mv: Placement) -> Option<ModeSwitch>;
    fn new_piece(&mut self, options: &BotOptions, piece: Piece);
    fn suggest(&self, options: &BotOptions) -> Vec<Placement>;
    fn do_work(&self, options: &BotOptions) -> Statistics;
}

enum ModeSwitch {
    Freestyle,
}

impl Bot {
    pub fn new(options: BotOptions, root: GameState, queue: &[Piece]) -> Self {
        Bot {
            current: root,
            queue: queue.iter().copied().collect(),
            mode: Freestyle::new(&options, root, queue).into(),
            options,
        }
    }

    pub fn advance(&mut self, mv: Placement) {
        puffin::profile_function!();
        self.current.advance(self.queue.pop_front().unwrap(), mv);
        if let Some(to) = self.mode.advance(&self.options, mv) {
            self.switch(to);
        };
    }

    pub fn new_piece(&mut self, piece: Piece) {
        puffin::profile_function!();
        self.queue.push_back(piece);
        self.mode.new_piece(&self.options, piece);
    }

    pub fn suggest(&self) -> Vec<Placement> {
        puffin::profile_function!();
        self.mode.suggest(&self.options)
    }

    pub fn do_work(&self) -> Statistics {
        puffin::profile_function!();
        self.mode.do_work(&self.options)
    }

    fn switch(&mut self, to: ModeSwitch) {
        puffin::profile_function!();
        match to {
            ModeSwitch::Freestyle => {
                self.mode =
                    Freestyle::new(&self.options, self.current, self.queue.make_contiguous()).into()
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Statistics {
    pub nodes: u64,
    pub selections: u64,
    pub expansions: u64,
}

impl Default for Statistics {
    fn default() -> Self {
        Statistics {
            nodes: 0,
            selections: 0,
            expansions: 0,
        }
    }
}

impl Statistics {
    pub fn accumulate(&mut self, other: Self) {
        self.nodes += other.nodes;
        self.selections += other.selections;
        self.expansions += other.expansions;
    }
}
