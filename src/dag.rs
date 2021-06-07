use std::collections::HashMap;
use std::sync::atomic;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::RwLock;
use std::time::Instant;

use enum_map::EnumMap;
use enumset::EnumSet;
use once_cell::sync::Lazy;

use crate::data::Placement;
use crate::data::{GameState, Piece};
use crate::profile::ProfileScope;

pub trait Evaluation: Ord + Copy + Default + std::ops::Add<Self::Reward, Output = Self> {
    type Reward: Copy;

    fn average(of: impl Iterator<Item = Option<Self>>) -> Self;
}

pub struct Dag<E: Evaluation> {
    root: GameState,
    top_layer: Box<Layer<E>>,
    last_advance: Instant,
    new_nodes: AtomicU64,
}

pub struct Selection<'a, E: Evaluation> {
    layers: Vec<&'a Layer<E>>,
    game_state: GameState,
    new_nodes: &'a AtomicU64,
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
    states: RwLock<HashMap<GameState, Node<E>>>,
    piece: Option<Piece>,
}

struct Node<E: Evaluation> {
    parents: Vec<(GameState, Placement)>,
    eval: E,
    children: Option<EnumMap<Piece, Vec<Child<E>>>>,
    expanding: AtomicBool,
}

#[derive(Clone, Copy, Debug)]
struct Child<E: Evaluation> {
    mv: Placement,
    reward: E::Reward,
    cached_eval: E,
}

impl<E: Evaluation> Dag<E> {
    pub fn new(root: GameState, queue: impl IntoIterator<Item = Piece>) -> Self {
        let mut top_layer = Layer::default();
        top_layer.states.get_mut().unwrap().insert(
            root,
            Node {
                parents: vec![],
                eval: E::default(),
                children: None,
                expanding: AtomicBool::new(false),
            },
        );

        let mut layer = &mut top_layer;
        for piece in queue {
            layer.piece = Some(piece);
            layer = &mut layer.next_layer;
        }

        Dag {
            root,
            top_layer: Box::new(top_layer),
            last_advance: Instant::now(),
            new_nodes: AtomicU64::new(0),
        }
    }

    pub fn advance(&mut self, mv: Placement) {
        let now = Instant::now();
        eprintln!(
            "{:.0} nodes/second",
            *self.new_nodes.get_mut() as f64 / now.duration_since(self.last_advance).as_secs_f64()
        );
        self.last_advance = now;
        *self.new_nodes.get_mut() = 0;

        let top_layer = std::mem::take(&mut *self.top_layer);
        self.root.advance(
            top_layer.piece.expect("cannot advance without next piece"),
            mv,
        );
        Lazy::force(&top_layer.next_layer);
        self.top_layer = Lazy::into_value(top_layer.next_layer).unwrap();
        self.top_layer
            .states
            .get_mut()
            .unwrap()
            .entry(self.root)
            .or_insert(Node {
                parents: vec![],
                eval: E::default(),
                children: None,
                expanding: AtomicBool::new(false),
            });
    }

    pub fn add_piece(&mut self, piece: Piece) {
        let mut layer = &mut self.top_layer;
        loop {
            if layer.piece.is_none() {
                layer.piece = Some(piece);
                return;
            }
            layer = &mut layer.next_layer;
        }
    }

    pub fn suggest(&self) -> Vec<Placement> {
        let states = self.top_layer.states.read().unwrap();
        let children = match &states.get(&self.root).unwrap().children {
            Some(children) => children,
            None => return vec![],
        };

        let mut candidates: Vec<&_> = vec![];
        match self.top_layer.piece {
            Some(next) => {
                candidates.extend(children[next].first());
                if next != self.root.reserve {
                    candidates.extend(children[self.root.reserve].first());
                }
            }
            None => {
                for piece in self.root.bag {
                    candidates.extend(children[piece].first());
                }
                if !self.root.bag.contains(self.root.reserve) {
                    candidates.extend(children[self.root.reserve].first());
                }
            }
        };
        candidates.sort_by(|a, b| a.cached_eval.partial_cmp(&b.cached_eval).unwrap().reverse());

        candidates.into_iter().map(|c| c.mv).collect()
    }

    pub fn select(&self) -> Option<Selection<E>> {
        let _scope = ProfileScope::new("select");

        let mut layers = vec![&*self.top_layer];
        let mut game_state = self.root;
        loop {
            let &layer = layers.last().unwrap();
            let guard = layer.states.read().unwrap();
            let node = guard.get(&game_state).expect("Link to non-existent node?");

            let children = match &node.children {
                None => {
                    if node.expanding.swap(true, atomic::Ordering::Acquire) {
                        return None;
                    } else {
                        return Some(Selection {
                            layers,
                            game_state,
                            new_nodes: &self.new_nodes,
                        });
                    }
                }
                Some(children) => children,
            };

            // TODO: draw from bag
            let next = layer.piece?;

            // TODO: monte-carlo selection
            let choice = children[next].first()?.mv;

            game_state.advance(next, choice);

            layers.push(&layer.next_layer);
        }
    }
}

impl<E: Evaluation> Selection<'_, E> {
    pub fn state(&self) -> (GameState, Option<Piece>) {
        (self.game_state, self.layers.last().unwrap().piece)
    }

    pub fn expand(self, children: impl IntoIterator<Item = ChildData<E>>) {
        let scope = ProfileScope::new("expand");

        let mut layers = self.layers;
        let start_layer = layers.pop().unwrap();

        let mut childs = EnumMap::<_, Vec<_>>::default();

        let mut next_states = start_layer.next_layer.states.write().unwrap();
        for child in children {
            let node = next_states.entry(child.resulting_state).or_insert(Node {
                parents: vec![],
                eval: child.eval,
                children: None,
                expanding: AtomicBool::new(false),
            });
            node.parents.push((self.game_state, child.mv));
            childs[child.mv.location.piece].push(Child {
                mv: child.mv,
                cached_eval: node.eval + child.reward,
                reward: child.reward,
            });
        }
        self.new_nodes.fetch_add(
            childs.values().map(|l| l.len() as u64).sum(),
            atomic::Ordering::Relaxed,
        );

        for list in childs.values_mut() {
            list.sort_by(|a, b| a.cached_eval.cmp(&b.cached_eval).reverse());
        }

        let mut next = vec![];

        let mut states = start_layer.states.write().unwrap();
        let node = states.get_mut(&self.game_state).unwrap();
        node.children = Some(childs);
        for &(parent_state, mv) in &node.parents {
            next.push((parent_state, mv, self.game_state));
        }

        drop(next_states);
        drop(states);

        drop(scope);
        let _scope = ProfileScope::new("backprop");

        let mut prev_layer = start_layer;
        while let Some(layer) = layers.pop() {
            let mut next_up = vec![];

            for (parent, placement, child) in next {
                let mut guard = layer.states.write().unwrap();
                let node = guard.get_mut(&parent).unwrap();
                let child_eval = prev_layer.states.read().unwrap().get(&child).unwrap().eval;

                let children = node.children.as_mut().unwrap();
                let list = &mut children[placement.location.piece];

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
                } else if index < list.len() - 1
                    && list[index + 1].cached_eval > list[index].cached_eval
                {
                    // Shift down until the list is in order
                    let hole = list[index];
                    while index < list.len() - 1 && list[index + 1].cached_eval > hole.cached_eval {
                        list[index] = list[index + 1];
                        index += 1;
                    }
                    list[index] = hole;
                }

                if index == 0 {
                    let next_possibilities = match layer.piece {
                        Some(p) => EnumSet::only(p),
                        None => parent.bag,
                    };

                    let best_for = |p: Piece| children[p].first().map(|c| c.cached_eval);

                    let eval = E::average(
                        next_possibilities
                            .iter()
                            .map(|p| best_for(p).max(best_for(parent.reserve))),
                    );

                    if node.eval != eval {
                        node.eval = eval;

                        for &(ps, mv) in &node.parents {
                            next_up.push((ps, mv, parent));
                        }
                    }
                }
            }

            next = next_up;
            prev_layer = layer;

            if next.is_empty() {
                break;
            }
        }
    }
}
