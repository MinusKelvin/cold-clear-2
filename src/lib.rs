use std::convert::Infallible;
use std::sync::Arc;

use bot::{BotConfig, BotOptions};
use enumset::EnumSet;
use futures::prelude::*;
use tbp::randomizer::{Bag, GeneralBag, RandomizerState};
use tbp::{bot_msg, frontend_msg, BotMessage, FrontendMessage, MaybeUnknown};

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
            bot_msg::Info::new(
                "Cold Clear 2".to_owned(),
                "MinusKelvin".to_owned(),
                env!("CARGO_PKG_VERSION").to_owned(),
                vec![],
            )
            .into(),
        )
        .await
        .unwrap();

    let bot = Arc::new(BotSyncronizer::new());

    spawn_workers(&bot);

    let mut waiting_on_first_piece = None;

    while let Some(msg) = incoming.next().await {
        match msg {
            FrontendMessage::Start(mut start) => {
                if let RandomizerState::SevenBag(bag) = start.randomizer {
                    let mut bagged = GeneralBag::new(Bag::new(), Bag::new());
                    bagged.filled_bag.i = 1;
                    bagged.filled_bag.o = 1;
                    bagged.filled_bag.t = 1;
                    bagged.filled_bag.l = 1;
                    bagged.filled_bag.j = 1;
                    bagged.filled_bag.s = 1;
                    bagged.filled_bag.z = 1;
                    if bag.bag_state.is_empty() {
                        bagged.current_bag = bagged.filled_bag.clone();
                    }
                    for p in bag.bag_state {
                        bagged.current_bag[p] = 1;
                    }
                    start.randomizer = bagged.into();
                }
                if start.hold.is_none() && start.queue.is_empty() {
                    waiting_on_first_piece = Some(start);
                } else {
                    bot.start(create_bot(start, config.clone()).unwrap());
                }
            }
            FrontendMessage::Stop(_) => {
                bot.stop();
                waiting_on_first_piece = None;
            }
            FrontendMessage::Suggest(_) => {
                if let Some((results, move_info)) = bot.suggest() {
                    let mut suggestion =
                        bot_msg::Suggestion::new(results.into_iter().map(Into::into).collect());
                    suggestion.move_info = move_info;
                    outgoing.send(suggestion.into()).await.unwrap();
                }
            }
            FrontendMessage::Play(play) => {
                bot.advance(play.mv.try_into().unwrap());
                puffin::GlobalProfiler::lock().new_frame();
            }
            FrontendMessage::NewPiece(new_piece) => {
                if let Some(mut start) = waiting_on_first_piece.take() {
                    if let RandomizerState::GeneralBag(bagged) = &mut start.randomizer {
                        if let Some(piece) = new_piece.piece.clone().known() {
                            bagged.current_bag[piece] = 0;
                        }
                    }
                    start.queue.push(new_piece.piece);
                    bot.start(create_bot(start, config.clone()).unwrap());
                } else {
                    bot.new_piece(new_piece.piece.try_into().unwrap());
                }
            }
            FrontendMessage::Rules(_) => {
                outgoing
                    .send(bot_msg::Ready::default().into())
                    .await
                    .unwrap();
            }
            FrontendMessage::Quit(_) => break,
            _ => {}
        }
    }
}

fn create_bot(
    start: frontend_msg::Start,
    config: Arc<BotConfig>,
) -> Result<Bot, convert::ConvertError> {
    let mut queue = start.queue.into_iter().map(TryInto::try_into);
    let reserve = start
        .hold
        .map_or_else(|| queue.next().unwrap(), TryInto::try_into)?;
    let queue = queue.collect::<Result<Vec<_>, _>>()?;

    const TBP_PIECES: [tbp::data::Piece; 7] = [
        tbp::data::Piece::I,
        tbp::data::Piece::O,
        tbp::data::Piece::T,
        tbp::data::Piece::L,
        tbp::data::Piece::J,
        tbp::data::Piece::S,
        tbp::data::Piece::Z,
    ];

    let bag;
    let speculate;
    match start.randomizer {
        RandomizerState::GeneralBag(bagged)
            if TBP_PIECES.iter().all(|p| bagged.filled_bag[p.clone()] == 1) =>
        {
            let mut bs = TBP_PIECES
                .iter()
                .filter(|&p| bagged.current_bag[p.clone()] == 1)
                .map(|p| Piece::try_from(MaybeUnknown::Known(p.clone())).unwrap())
                .collect::<EnumSet<_>>();
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

    Ok(Bot::new(BotOptions { speculate, config }, state, &queue))
}

fn spawn_workers(bot: &Arc<BotSyncronizer>) {
    for _ in 0..4 {
        let bot = bot.clone();
        std::thread::spawn(move || bot.work_loop());
    }
}
