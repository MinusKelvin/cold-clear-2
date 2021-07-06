use std::collections::VecDeque;

use enum_dispatch::enum_dispatch;

use crate::data::{GameState, Piece, Placement};

mod freestyle;

use self::freestyle::Freestyle;

pub struct Bot {
    options: BotOptions,
    current: GameState,
    queue: VecDeque<Piece>,
    mode: ModeEnum,
}

#[derive(Debug)]
pub struct BotOptions {
    pub speculate: bool,
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
    fn do_work(&self, options: &BotOptions);
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
        self.current.advance(self.queue.pop_front().unwrap(), mv);
        if let Some(to) = self.mode.advance(&self.options, mv) {
            self.switch(to);
        };
    }

    pub fn new_piece(&mut self, piece: Piece) {
        self.queue.push_back(piece);
        self.mode.new_piece(&self.options, piece);
    }

    pub fn suggest(&self) -> Vec<Placement> {
        self.mode.suggest(&self.options)
    }

    pub fn do_work(&self) {
        self.mode.do_work(&self.options);
    }

    fn switch(&mut self, to: ModeSwitch) {
        match to {
            ModeSwitch::Freestyle => {
                self.mode =
                    Freestyle::new(&self.options, self.current, self.queue.make_contiguous()).into()
            }
        }
    }
}
