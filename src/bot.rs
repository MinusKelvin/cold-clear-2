use std::collections::VecDeque;

use enum_dispatch::enum_dispatch;

use crate::data::{GameState, Piece, Placement};

mod freestyle;

use self::freestyle::Freestyle;

pub struct Bot {
    current: GameState,
    queue: VecDeque<Piece>,
    mode: ModeEnum,
}

#[enum_dispatch]
enum ModeEnum {
    Freestyle,
}

#[enum_dispatch(ModeEnum)]
trait Mode {
    fn advance(&mut self, mv: Placement) -> Option<ModeSwitch>;
    fn new_piece(&mut self, piece: Piece);
    fn suggest(&self) -> Vec<Placement>;
    fn do_work(&self);
}

enum ModeSwitch {
    Freestyle,
}

impl Bot {
    pub fn new(root: GameState, queue: &[Piece]) -> Self {
        Bot {
            current: root,
            queue: queue.iter().copied().collect(),
            mode: Freestyle::new(root, queue).into(),
        }
    }

    pub fn advance(&mut self, mv: Placement) {
        self.current.advance(self.queue.pop_front().unwrap(), mv);
        if let Some(to) = self.mode.advance(mv) {
            self.switch(to);
        };
    }

    pub fn new_piece(&mut self, piece: Piece) {
        self.queue.push_back(piece);
        self.mode.new_piece(piece);
    }

    pub fn suggest(&self) -> Vec<Placement> {
        self.mode.suggest()
    }

    pub fn do_work(&self) {
        self.mode.do_work();
    }

    fn switch(&mut self, to: ModeSwitch) {
        match to {
            ModeSwitch::Freestyle => {
                self.mode = Freestyle::new(
                    self.current,
                    self.queue.make_contiguous(),
                ).into()
            }
        }
    }
}
