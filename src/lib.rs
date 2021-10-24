use std::convert::Infallible;
use std::sync::Arc;

use bot::BotOptions;
use enumset::EnumSet;
use futures::prelude::*;
use tbp::randomizer::RandomizerState;
use tbp::{BotMessage, FrontendMessage};

use crate::bot::Bot;
use crate::data::{GameState, Piece};
use crate::sync::BotSyncronizer;

mod bot;
mod convert;
mod dag;
#[macro_use]
pub mod data;
mod map;
pub mod movegen;
mod sync;

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

    let bot = Arc::new(BotSyncronizer::new());

    spawn_workers(&bot);

    let mut waiting_on_first_piece = None;

    while let Some(msg) = incoming.next().await {
        match msg {
            FrontendMessage::Start {
                hold, ref queue, ..
            } => {
                if hold.is_none() && queue.is_empty() {
                    waiting_on_first_piece = Some(msg);
                } else {
                    bot.start(create_bot(msg));
                }
            }
            FrontendMessage::Stop => {
                bot.stop();
                waiting_on_first_piece = None;
            }
            FrontendMessage::Suggest => {
                if let Some((results, move_info)) = bot.suggest() {
                    outgoing
                        .send(BotMessage::Suggestion {
                            moves: results.into_iter().map(Into::into).collect(),
                            move_info,
                        })
                        .await
                        .unwrap();
                }
            }
            FrontendMessage::Play { mv } => {
                bot.advance(mv.into());
                puffin::GlobalProfiler::lock().new_frame();
            }
            FrontendMessage::NewPiece { piece } => {
                if let Some(mut msg) = waiting_on_first_piece.take() {
                    if let FrontendMessage::Start { queue, .. } = &mut msg {
                        queue.push(piece);
                        bot.start(create_bot(msg));
                    } else {
                        unreachable!()
                    }
                } else {
                    bot.new_piece(piece.into());
                }
            }
            FrontendMessage::Rules { randomizer: _ } => {
                outgoing.send(BotMessage::Ready).await.unwrap();
            }
            FrontendMessage::Quit => break,
        }
    }
}

fn create_bot(start_msg: FrontendMessage) -> Bot {
    if let FrontendMessage::Start {
        hold,
        queue,
        combo,
        back_to_back,
        board,
        randomizer,
    } = start_msg
    {
        let mut queue = queue.into_iter().map(Into::into);
        let reserve = hold.map_or_else(|| queue.next().unwrap(), Into::into);
        let queue: Vec<_> = queue.collect();

        let bag;
        let speculate;
        match randomizer {
            RandomizerState::SevenBag { bag_state } => {
                let mut bs: EnumSet<_> = bag_state.into_iter().map(Piece::from).collect();
                for &p in queue.iter().rev() {
                    if bs == EnumSet::all() {
                        bs = EnumSet::empty();
                    }
                    bs.insert(p);
                }
                bag = bs;
                speculate = true;
            }
            _ => {
                bag = EnumSet::all();
                speculate = false;
            }
        };

        let state = GameState {
            reserve,
            back_to_back,
            combo: if combo > 255 { 255 } else { combo as u8 },
            bag,
            board: board.into(),
        };

        Bot::new(BotOptions { speculate }, state, &queue)
    } else {
        unreachable!();
    }
}

fn spawn_workers(bot: &Arc<BotSyncronizer>) {
    for _ in 0..4 {
        let bot = bot.clone();
        std::thread::spawn(move || bot.work_loop());
    }
}
