use std::sync::atomic::{self, AtomicBool};

use bumpalo_herd::{Herd, Member};
use enum_map::EnumMap;
use rand::prelude::*;

use crate::data::{GameState, Piece, Placement};
use crate::map::StateMap;

use super::{
    update_child, BackpropUpdate, Child, ChildData, Evaluation, LayerCommon, SelectResult,
};

pub(super) struct Layer<'bump, E: Evaluation> {
    pub states: StateMap<Node<'bump, E>>,
    pub piece: Piece,
}

pub(super) struct Node<'bump, E: Evaluation> {
    pub parents: &'bump [(u64, Placement, Piece)],
    pub eval: E,
    pub children: Option<&'bump mut [Child<E>]>,
    pub expanding: AtomicBool,
}

impl<'bump, E: Evaluation> Layer<'bump, E> {
    pub fn initialize_root(&self, root: &GameState) {
        let _ = self.states.get_or_insert_with(root, || Node {
            parents: &[],
            eval: E::default(),
            children: None,
            expanding: AtomicBool::new(false),
        });
    }

    pub fn suggest(&self, state: &GameState) -> Vec<Placement> {
        puffin::profile_function!();
        let node = self.states.get(state).unwrap();
        let children = match &node.children {
            Some(children) => children,
            None => return vec![],
        };

        let mut candidates: Vec<&_> = vec![];
        candidates.extend(children.first());
        candidates.sort_by(|a, b| a.cached_eval.partial_cmp(&b.cached_eval).unwrap().reverse());

        candidates.into_iter().map(|c| c.mv).collect()
    }

    pub fn select(&self, game_state: &GameState, exploration: f64) -> SelectResult {
        puffin::profile_function!();
        let node = self
            .states
            .get(game_state)
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

        let s: f64 = thread_rng().gen();
        let i = ((-s.ln() / exploration) % children.len() as f64) as usize;
        SelectResult::Advance(self.piece, children[i].mv)
    }

    pub fn get_eval(&self, raw: u64) -> E {
        self.states.get_raw(raw).unwrap().eval
    }

    pub fn create_node(
        &self,
        bump: &Member<'bump>,
        child: &ChildData<E>,
        parent: u64,
        speculation_piece: Piece,
    ) -> E {
        let mut node = self
            .states
            .get_or_insert_with(&child.resulting_state, || Node {
                parents: &[],
                eval: child.eval,
                children: None,
                expanding: AtomicBool::new(false),
            });
        node.parents = bump.alloc_slice_fill_with(node.parents.len() + 1, |i| {
            node.parents
                .get(i)
                .copied()
                .unwrap_or((parent, child.mv, speculation_piece))
        });
        node.eval
    }

    pub fn expand(
        &self,
        herd: &'bump Herd,
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
            let evals =
                next_layer
                    .kind
                    .create_nodes(&children[self.piece], parent_index, self.piece);
            for (child, eval) in children[self.piece].iter().zip(evals.into_iter()) {
                childs.push(Child {
                    mv: child.mv,
                    cached_eval: eval + child.reward,
                    reward: child.reward,
                });
            }
        }

        childs.sort_by(|a, b| a.cached_eval.cmp(&b.cached_eval).reverse());

        parent.eval = E::average(std::iter::once(childs.first().map(|c| c.cached_eval)));
        parent.children = Some(herd.get().alloc_slice_copy(&childs));

        let mut next = vec![];

        for &(grandparent, mv, speculation_piece) in parent.parents {
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

                    for &(parent, mv, speculation_piece) in parent.parents {
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
