use std::sync::atomic::{self, AtomicBool};

use enum_map::EnumMap;
use enumset::EnumSet;
use rand::prelude::*;
use smallvec::SmallVec;

use crate::data::{GameState, Piece, Placement};
use crate::map::StateMap;

use super::{update_child, BackpropUpdate, Child, ChildData, Evaluation, Layer, SelectResult};

#[derive(Default)]
pub struct Raw<E: Evaluation> {
    states: StateMap<Node<E>>,
    piece: Option<Piece>,
}

struct Node<E: Evaluation> {
    parents: SmallVec<[(u64, Placement, Piece); 1]>,
    eval: E,
    children: Option<EnumMap<Piece, Box<[Child<E>]>>>,
    expanding: AtomicBool,
    // we need this info while backpropagating, but we don't have access to the game state then
    bag: EnumSet<Piece>,
    reserve: Piece,
}

impl<E: Evaluation> Raw<E> {
    pub fn initialize_root(&self, root: &GameState) {
        let _ = self.states.get_or_insert_with(root, || Node {
            parents: SmallVec::new(),
            eval: E::default(),
            children: None,
            expanding: AtomicBool::new(false),
            bag: root.bag,
            reserve: root.reserve,
        });
    }

    pub fn piece(&self) -> Option<Piece> {
        self.piece
    }

    pub fn suggest(&self, state: &GameState) -> Vec<Placement> {
        let node = self.states.get(&state).unwrap();
        let children = match &node.children {
            Some(children) => children,
            None => return vec![],
        };

        let mut candidates: Vec<&_> = vec![];
        match self.piece {
            Some(next) => {
                candidates.extend(children[next].first());
            }
            None => {
                for piece in state.bag {
                    candidates.extend(children[piece].first());
                }
            }
        };
        candidates.sort_by(|a, b| a.cached_eval.partial_cmp(&b.cached_eval).unwrap().reverse());

        candidates.into_iter().map(|c| c.mv).collect()
    }

    pub fn select(&self, game_state: &GameState) -> SelectResult {
        let node = self
            .states
            .get(&game_state)
            .expect("Link to non-existent node?");

        let children = match &node.children {
            None => {
                if node.expanding.swap(true, atomic::Ordering::Relaxed) {
                    return SelectResult::Failed;
                } else {
                    return SelectResult::Done;
                }
            }
            Some(children) => children,
        };

        let next = self.piece.unwrap_or_else(|| {
            let i = thread_rng().gen_range(0..game_state.bag.len());
            game_state.bag.iter().nth(i).unwrap()
        });

        if children[next].is_empty() {
            return SelectResult::Failed;
        }

        loop {
            let s: f64 = thread_rng().gen();
            let i = -s.log2() as usize;
            if i < children[next].len() {
                break SelectResult::Advance(next, children[next][i].mv);
            }
        }
    }

    pub fn despeculate(&mut self, piece: Piece) -> bool {
        let result = self.piece.is_none();
        if result {
            self.piece = Some(piece);
        }
        result
    }

    pub fn get_eval(&self, raw: u64) -> E {
        self.states.get_raw(raw).unwrap().eval
    }

    pub fn create_node(&self, child: &ChildData<E>, parent: u64, speculation_piece: Piece) -> E {
        let mut node = self
            .states
            .get_or_insert_with(&child.resulting_state, || Node {
                parents: SmallVec::new(),
                eval: child.eval,
                children: None,
                expanding: AtomicBool::new(false),
                bag: child.resulting_state.bag,
                reserve: child.resulting_state.reserve,
            });
        node.parents.push((parent, child.mv, speculation_piece));
        node.eval
    }

    pub fn expand(
        &self,
        next_layer: &Layer<E>,
        parent_state: GameState,
        children: EnumMap<Piece, Vec<ChildData<E>>>,
    ) -> Vec<BackpropUpdate> {
        let mut childs = EnumMap::<_, Vec<_>>::default();

        // We need to acquire the lock on the parent since the backprop routine needs the children
        // lists to exist, and they won't if we're still creating them
        let parent_index = self.states.index(&parent_state);
        let mut parent = self.states.get_raw_mut(parent_index).unwrap();

        for (speculation_piece, child) in children
            .into_iter()
            .flat_map(|(p, children)| children.into_iter().map(move |d| (p, d)))
        {
            let eval = next_layer
                .kind
                .create_node(&child, parent_index, speculation_piece);
            childs[speculation_piece].push(Child {
                mv: child.mv,
                cached_eval: eval + child.reward,
                reward: child.reward,
            });
        }

        for list in childs.values_mut() {
            list.sort_by(|a, b| a.cached_eval.cmp(&b.cached_eval).reverse());
        }

        let next_possibilities = match self.piece {
            Some(p) => EnumSet::only(p),
            None => parent.bag,
        };
        parent.eval = E::average(
            next_possibilities
                .iter()
                .map(|p| childs[p].first().map(|c| c.cached_eval)),
        );

        let mut boxed_slice_childs = EnumMap::default();
        for (k, v) in childs {
            boxed_slice_childs[k] = v.into_boxed_slice();
        }
        parent.children = Some(boxed_slice_childs);

        let mut next = vec![];

        for &(grandparent, mv, speculation_piece) in &parent.parents {
            next.push(BackpropUpdate {
                parent: grandparent,
                mv,
                speculation_piece,
                child: parent_index,
            });
        }

        next
    }

    pub fn backprop(
        &self,
        to_update: Vec<BackpropUpdate>,
        next_layer: &Layer<E>,
    ) -> Vec<BackpropUpdate> {
        let mut new_updates = vec![];

        for update in to_update {
            let mut parent = self.states.get_raw_mut(update.parent).unwrap();
            let child_eval = next_layer.kind.get_eval(update.child);

            let parent_bag = parent.bag;
            let parent_reserve = parent.reserve;
            let children = parent.children.as_mut().unwrap();
            let list = &mut children[update.speculation_piece];

            let is_best = update_child(list, update.mv, child_eval);

            if is_best {
                let next_possibilities = match self.piece {
                    Some(p) => EnumSet::only(p),
                    None => parent_bag,
                };

                let best_for = |p: Piece| children[p].first().map(|c| c.cached_eval);

                let eval = E::average(
                    next_possibilities
                        .iter()
                        .map(|p| best_for(p).max(best_for(parent_reserve))),
                );

                if parent.eval != eval {
                    parent.eval = eval;

                    for &(parent, mv, speculation_piece) in &parent.parents {
                        new_updates.push(BackpropUpdate {
                            parent,
                            mv,
                            speculation_piece,
                            child: update.parent,
                        });
                    }
                }
            }
        }

        new_updates
    }
}
