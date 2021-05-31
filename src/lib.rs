use std::convert::Infallible;
use std::sync::Arc;

use enumset::EnumSet;
use futures::prelude::*;
use tbp::{BotMessage, FrontendMessage};

use crate::dag::Dag;
use crate::data::GameState;
use crate::sharing::SharedState;

mod convert;
mod dag;
pub mod data;
mod sharing;

pub async fn run(
    mut incoming: impl Stream<Item = FrontendMessage> + Unpin,
    mut outgoing: impl Sink<BotMessage, Error = Infallible> + Unpin,
) {
    outgoing
        .send(BotMessage::Info {
            name: "Cold Clear 2".to_owned(),
            author: "MinusKelvin".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            features: tbp::Feature::enabled(),
        })
        .await
        .unwrap();

    let bot = Arc::new(SharedState::<Dag>::new());

    spawn_workers(&bot);

    let mut waiting_on_first_piece = None;

    while let Some(msg) = incoming.next().await {
        match msg {
            FrontendMessage::Start {
                hold,
                queue,
                combo,
                back_to_back,
                board,
            } => {
                let board = board.into();
                let hold = hold.map(Into::into);
                let mut queue = queue.into_iter().map(Into::into);
                let combo = combo.min(20) as u8;
                match hold.or_else(|| queue.next()) {
                    Some(reserve) => {
                        bot.start(Dag::new(
                            GameState {
                                board,
                                reserve,
                                back_to_back,
                                combo,
                                bag: EnumSet::all() - reserve,
                            },
                            queue,
                        ));
                    }
                    None => {
                        bot.stop();
                        waiting_on_first_piece = Some((board, back_to_back, combo));
                    }
                }
            }
            FrontendMessage::Stop => {
                bot.stop();
                waiting_on_first_piece = None;
            }
            FrontendMessage::Suggest => {
                if let Some(results) = bot.read_op_if_exists(|state| state.suggest()) {
                    outgoing
                        .send(BotMessage::Suggestion {
                            moves: results.into_iter().map(Into::into).collect(),
                        })
                        .await
                        .unwrap();
                }
            }
            FrontendMessage::Play { mv } => {
                bot.write_op_if_exists(|state| state.advance(mv.into()));
            }
            FrontendMessage::NewPiece { piece } => {
                let piece = piece.into();
                if let Some((board, back_to_back, combo)) = waiting_on_first_piece.take() {
                    bot.start(Dag::new(
                        GameState {
                            board,
                            back_to_back,
                            combo,
                            reserve: piece,
                            bag: EnumSet::all() - piece,
                        },
                        std::iter::empty(),
                    ))
                } else {
                    bot.write_op_if_exists(|state| state.add_piece(piece));
                }
            }
            FrontendMessage::Rules {} => {
                outgoing.send(BotMessage::Ready).await.unwrap();
            }
            FrontendMessage::Quit => break,
        }
    }
}

fn spawn_workers(bot: &Arc<SharedState<Dag>>) {
    let bot = bot.clone();
    std::thread::spawn(move || loop {
        bot.read_op(|dag| {
            if let Some(node) = dag.select() {
                todo!();
                node.expand(std::iter::empty())
            }
        });
    });
}
