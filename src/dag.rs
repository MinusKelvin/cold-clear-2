use enum_map::EnumMap;
use once_cell::sync::Lazy;

use crate::data::Placement;
use crate::data::{GameState, Piece};

mod layer;

pub trait Evaluation: Ord + Copy + Default + std::ops::Add<Self::Reward, Output = Self> {
    type Reward: Copy;

    fn average(of: impl Iterator<Item = Option<Self>>) -> Self;
}

pub struct Dag<E: Evaluation> {
    root: GameState,
    top_layer: Box<Layer<E>>,
}

pub struct Selection<'a, E: Evaluation> {
    layers: Vec<&'a Layer<E>>,
    game_state: GameState,
}

pub struct ChildData<E: Evaluation> {
    pub resulting_state: GameState,
    pub mv: Placement,
    pub eval: E,
    pub reward: E::Reward,
}

#[derive(Default)]
struct Layer<E: Evaluation> {
    next_layer: Lazy<Box<Layer<E>>>,
    kind: layer::Raw<E>,
}

#[derive(Clone, Copy, Debug)]
struct Child<E: Evaluation> {
    mv: Placement,
    reward: E::Reward,
    cached_eval: E,
}

enum SelectResult {
    Failed,
    Done,
    Advance(Piece, Placement),
}

struct BackpropUpdate {
    parent: u64,
    speculation_piece: Piece,
    mv: Placement,
    child: u64,
}

impl<E: Evaluation> Dag<E> {
    pub fn new(root: GameState, queue: &[Piece]) -> Self {
        let mut top_layer = Layer::default();
        top_layer.kind.initialize_root(&root);

        let mut layer = &mut top_layer;
        for &piece in queue {
            layer.kind.despeculate(piece);
            layer = &mut layer.next_layer;
        }

        Dag {
            root,
            top_layer: Box::new(top_layer),
        }
    }

    pub fn advance(&mut self, mv: Placement) {
        let top_layer = std::mem::take(&mut *self.top_layer);
        self.root.advance(
            top_layer
                .kind
                .piece()
                .expect("cannot advance without next piece"),
            mv,
        );
        Lazy::force(&top_layer.next_layer);
        self.top_layer = Lazy::into_value(top_layer.next_layer).unwrap();
        self.top_layer.kind.initialize_root(&self.root);
    }

    pub fn add_piece(&mut self, piece: Piece) {
        let mut layer = &mut self.top_layer;
        loop {
            if layer.kind.despeculate(piece) {
                return;
            }
            layer = &mut layer.next_layer;
        }
    }

    pub fn suggest(&self) -> Vec<Placement> {
        self.top_layer.kind.suggest(&self.root)
    }

    pub fn select(&self, speculate: bool) -> Option<Selection<E>> {
        let mut layers = vec![&*self.top_layer];
        let mut game_state = self.root;
        loop {
            let &layer = layers.last().unwrap();

            if !speculate && layer.kind.piece().is_none() {
                return None;
            }

            match layer.kind.select(&game_state) {
                SelectResult::Failed => return None,
                SelectResult::Done => return Some(Selection { layers, game_state }),
                SelectResult::Advance(next, placement) => {
                    game_state.advance(next, placement);
                    layers.push(&layer.next_layer);
                }
            }
        }
    }
}

impl<E: Evaluation> Selection<'_, E> {
    pub fn state(&self) -> (GameState, Option<Piece>) {
        (self.game_state, self.layers.last().unwrap().kind.piece())
    }

    pub fn expand(self, children: EnumMap<Piece, Vec<ChildData<E>>>) {
        let mut layers = self.layers;
        let start_layer = layers.pop().unwrap();
        let mut next = start_layer
            .kind
            .expand(&start_layer.next_layer, self.game_state, children);

        let mut next_layer = start_layer;
        while let Some(layer) = layers.pop() {
            next = layer.kind.backprop(next, next_layer);
            next_layer = layer;

            if next.is_empty() {
                break;
            }
        }
    }
}

fn update_child<E: Evaluation>(list: &mut [Child<E>], placement: Placement, child_eval: E) -> bool {
    let mut index = list
        .iter()
        .enumerate()
        .find_map(|(i, c)| (c.mv == placement).then(|| i))
        .unwrap();

    list[index].cached_eval = child_eval + list[index].reward;

    if index > 0 && list[index - 1].cached_eval < list[index].cached_eval {
        // Shift up until the list is in order
        let hole = list[index];
        while index > 0 && list[index - 1].cached_eval < hole.cached_eval {
            list[index] = list[index - 1];
            index -= 1;
        }
        list[index] = hole;
    } else if index < list.len() - 1 && list[index + 1].cached_eval > list[index].cached_eval {
        // Shift down until the list is in order
        let hole = list[index];
        while index < list.len() - 1 && list[index + 1].cached_eval > hole.cached_eval {
            list[index] = list[index + 1];
            index += 1;
        }
        list[index] = hole;
    }

    index == 0
}
