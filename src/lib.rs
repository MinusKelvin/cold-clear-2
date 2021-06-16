use std::convert::Infallible;
use std::sync::Arc;

use bot::BotOptions;
use enumset::EnumSet;
use futures::prelude::*;
use tbp::randomizer::RandomizerState;
use tbp::{BotMessage, FrontendMessage};

use crate::bot::Bot;
use crate::data::{GameState, Piece};
use crate::sharing::SharedState;

mod bot;
mod convert;
mod dag;
mod data;
mod map;
mod movegen;
mod sharing;

pub async fn run(
    mut incoming: impl Stream<Item = FrontendMessage> + Unpin,
    mut outgoing: impl Sink<BotMessage, Error = Infallible> + Unpin,
) {
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        let report = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open("profile.txt")
            .unwrap();
        let mut report = std::io::BufWriter::new(report);
        if let Some(msg) = info.payload().downcast_ref::<&str>() {
            writeln!(report, "{}", msg).unwrap();
        } else if let Some(msg) = info.payload().downcast_ref::<String>() {
            writeln!(report, "{}", msg).unwrap();
        } else {
            writeln!(report, "Unknown panic payload type").unwrap();
        }
    }));

    outgoing
        .send(BotMessage::Info {
            name: "Cold Clear 2".to_owned(),
            author: "MinusKelvin".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            features: tbp::Feature::enabled(),
        })
        .await
        .unwrap();

    let bot = Arc::new(SharedState::<Bot>::new());

    profile::setup_thread();
    spawn_workers(&bot);

    let mut waiting_on_first_piece = None;

    while let Some(msg) = incoming.next().await {
        match msg {
            FrontendMessage::Start {
                hold, ref queue, ..
            } => {
                if hold.is_none() && queue.len() == 0 {
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
                if let Some(mut msg) = waiting_on_first_piece.take() {
                    if let FrontendMessage::Start { queue, .. } = &mut msg {
                        queue.push(piece);
                        bot.start(create_bot(msg));
                    } else {
                        unreachable!()
                    }
                } else {
                    let piece = piece.into();
                    bot.write_op_if_exists(|state| state.new_piece(piece));
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

fn spawn_workers(bot: &Arc<SharedState<Bot>>) {
    for _ in 0..1 {
        let bot = bot.clone();
        std::thread::spawn(move || {
            profile::setup_thread();
            loop {
                bot.read_op(|bot| bot.do_work());
            }
        });
    }
}

#[cfg(feature = "profile")]
mod profile;

#[cfg(not(feature = "profile"))]
mod profile {
    use std::io::Write;

    pub struct ProfileScope {
        _priv: (),
    }

    impl ProfileScope {
        pub fn new(_name: &'static str) -> Self {
            ProfileScope { _priv: () }
        }
    }

    impl Drop for ProfileScope {
        fn drop(&mut self) {}
    }

    pub fn setup_thread() {}

    static TOTALS: once_cell::sync::Lazy<parking_lot::Mutex<(u64, std::time::Duration)>> =
        once_cell::sync::Lazy::new(Default::default);

    pub fn profiling_frame_end(nodes: u64, time: std::time::Duration) {
        let report = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open("profile.txt")
            .unwrap();
        let mut report = std::io::BufWriter::new(report);
        let mut data = TOTALS.lock();
        data.0 += nodes;
        data.1 += time;
        writeln!(
            report,
            "{} nodes in {:.2?} ({:.1} kn/s, {:.1} kn/s average)",
            nodes,
            time,
            nodes as f64 / time.as_secs_f64() / 1000.0,
            data.0 as f64 / data.1.as_secs_f64() / 1000.0
        )
        .unwrap();
    }
}
