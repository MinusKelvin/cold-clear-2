use std::sync::atomic::{self, AtomicBool};

use enum_map::EnumMap;
use rand::prelude::*;
use smallvec::SmallVec;

use crate::data::{GameState, Piece, Placement};
use crate::map::StateMap;

use super::{
    update_child, BackpropUpdate, Child, ChildData, Evaluation, LayerCommon, SelectResult,
};

pub(super) struct Layer<E: Evaluation> {
    pub states: StateMap<Node<E>>,
    pub piece: Piece,
}

pub(super) struct Node<E: Evaluation> {
    pub parents: SmallVec<[(u64, Placement, Piece); 1]>,
    pub eval: E,
    pub children: Option<Box<[Child<E>]>>,
    pub expanding: AtomicBool,
}

impl<E: Evaluation> Layer<E> {
    pub fn initialize_root(&self, root: &GameState) {
        let _ = self.states.get_or_insert_with(root, || Node {
            parents: SmallVec::new(),
            eval: E::default(),
            children: None,
            expanding: AtomicBool::new(false),
        });
    }

    pub fn suggest(&self, state: &GameState) -> Vec<Placement> {
        puffin::profile_function!();
        let node = self.states.get(&state).unwrap();
        let children = match &node.children {
            Some(children) => children,
            None => return vec![],
        };

        let mut candidates: Vec<&_> = vec![];
        candidates.extend(children.first());
        candidates.sort_by(|a, b| a.cached_eval.partial_cmp(&b.cached_eval).unwrap().reverse());

        candidates.into_iter().map(|c| c.mv).collect()
    }

    pub fn select(&self, game_state: &GameState) -> SelectResult {
        puffin::profile_function!();
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

        if children.is_empty() {
            return SelectResult::Failed;
        }

        loop {
            let s: f64 = thread_rng().gen();
            let i = -s.log2() as usize;
            if i < children.len() {
                break SelectResult::Advance(self.piece, children[i].mv);
            }
        }
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
            });
        node.parents.push((parent, child.mv, speculation_piece));
        node.eval
    }

    pub fn expand(
        &self,
        next_layer: &LayerCommon<E>,
        parent_state: GameState,
        children: EnumMap<Piece, Vec<ChildData<E>>>,
    ) -> Vec<BackpropUpdate> {
        puffin::profile_function!();
        let mut childs = Vec::with_capacity(children[self.piece].len());

        // We need to acquire the lock on the parent since the backprop routine needs the children
        // lists to exist, and they won't if we're still creating them
        let parent_index = self.states.index(&parent_state);
        let mut parent = self.states.get_raw_mut(parent_index).unwrap();

        {
            puffin::profile_scope!("create nodes");
            for child in &children[self.piece] {
                let eval = next_layer.kind.create_node(child, parent_index, self.piece);
                childs.push(Child {
                    mv: child.mv,
                    cached_eval: eval + child.reward,
                    reward: child.reward,
                });
            }
        }

        childs.sort_by(|a, b| a.cached_eval.cmp(&b.cached_eval).reverse());

        parent.eval = E::average(std::iter::once(childs.first().map(|c| c.cached_eval)));
        parent.children = Some(childs.into_boxed_slice());

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
        next_layer: &LayerCommon<E>,
    ) -> Vec<BackpropUpdate> {
        puffin::profile_function!();
        let mut new_updates = vec![];

        for update in to_update {
            if update.speculation_piece != self.piece {
                continue;
            }

            let mut parent = self.states.get_raw_mut(update.parent).unwrap();
            let child_eval = next_layer.kind.get_eval(update.child);

            let children = parent.children.as_mut().unwrap();

            let is_best = update_child(children, update.mv, child_eval);

            if is_best {
                let eval = children[0].cached_eval;

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
