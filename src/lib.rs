use std::convert::Infallible;
use std::sync::Arc;

use bot::{BotConfig, BotOptions};
use enumset::EnumSet;
use futures::prelude::*;
use tbp::randomizer::RandomizerState;
use tbp::{bot_msg, frontend_msg, BotMessage, FrontendMessage};

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
    config: Arc<BotConfig>,
) {
    outgoing
        .send(
            bot_msg::Info {
                name: "Cold Clear 2".to_owned(),
                author: "MinusKelvin".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                features: tbp::Feature::enabled(),
                rest: Default::default(),
            }
            .into(),
        )
        .await
        .unwrap();

    let bot = Arc::new(BotSyncronizer::new());

    spawn_workers(&bot);

    let mut waiting_on_first_piece = None;

    while let Some(msg) = incoming.next().await {
        match msg {
            FrontendMessage::Start(start) => {
                if start.hold.is_none() && start.queue.is_empty() {
                    waiting_on_first_piece = Some(start);
                } else {
                    bot.start(create_bot(start, config.clone()));
                }
            }
            FrontendMessage::Stop(_) => {
                bot.stop();
                waiting_on_first_piece = None;
            }
            FrontendMessage::Suggest(_) => {
                if let Some((results, move_info)) = bot.suggest() {
                    outgoing
                        .send(
                            bot_msg::Suggestion {
                                moves: results.into_iter().map(Into::into).collect(),
                                move_info,
                                rest: Default::default(),
                            }
                            .into(),
                        )
                        .await
                        .unwrap();
                }
            }
            FrontendMessage::Play(play) => {
                bot.advance(play.mv.into());
                puffin::GlobalProfiler::lock().new_frame();
            }
            FrontendMessage::NewPiece(new_piece) => {
                if let Some(mut start) = waiting_on_first_piece.take() {
                    if let RandomizerState::SevenBag { bag_state } = &mut start.randomizer {
                        bag_state.retain(|p| p != &new_piece.piece);
                    }
                    start.queue.push(new_piece.piece);
                    bot.start(create_bot(start, config.clone()));
                } else {
                    bot.new_piece(new_piece.piece.into());
                }
            }
            FrontendMessage::Rules(_) => {
                outgoing
                    .send(bot_msg::Ready::default().into())
                    .await
                    .unwrap();
            }
            FrontendMessage::Quit(_) => break,
        }
    }
}

fn create_bot(start: frontend_msg::Start, config: Arc<BotConfig>) -> Bot {
    let mut queue = start.queue.into_iter().map(Into::into);
    let reserve = start.hold.map_or_else(|| queue.next().unwrap(), Into::into);
    let queue: Vec<_> = queue.collect();

    let bag;
    let speculate;
    match start.randomizer {
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
        back_to_back: start.back_to_back,
        combo: if start.combo > 255 {
            255
        } else {
            start.combo as u8
        },
        bag,
        board: start.board.into(),
    };

    Bot::new(BotOptions { speculate, config }, state, &queue)
}

fn spawn_workers(bot: &Arc<BotSyncronizer>) {
    for _ in 0..4 {
        let bot = bot.clone();
        std::thread::spawn(move || bot.work_loop());
    }
}
