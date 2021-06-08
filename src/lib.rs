use std::convert::Infallible;
use std::sync::Arc;

use enumset::EnumSet;
use futures::prelude::*;
use tbp::{BotMessage, FrontendMessage};

use crate::bot::Bot;
use crate::data::GameState;
use crate::sharing::SharedState;

mod bot;
mod convert;
mod dag;
mod data;
mod movegen;
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

    let bot = Arc::new(SharedState::<Bot>::new());

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
                        bot.start(Bot::new(
                            GameState {
                                board,
                                reserve,
                                back_to_back,
                                combo,
                                bag: EnumSet::all() - reserve,
                            },
                            &queue.collect::<Vec<_>>(),
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
                    bot.start(Bot::new(
                        GameState {
                            board,
                            back_to_back,
                            combo,
                            reserve: piece,
                            bag: EnumSet::all() - piece,
                        },
                        &[],
                    ))
                } else {
                    bot.write_op_if_exists(|state| state.new_piece(piece));
                }
            }
            FrontendMessage::Rules {} => {
                outgoing.send(BotMessage::Ready).await.unwrap();
            }
            FrontendMessage::Quit => break,
        }
    }
}

fn spawn_workers(bot: &Arc<SharedState<Bot>>) {
    let bot = bot.clone();
    std::thread::spawn(move || loop {
        bot.read_op(|bot| bot.do_work());
    });
}

#[cfg(feature = "profile")]
mod profile {
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::Mutex;
    use std::time::Duration;
    use std::time::Instant;

    use once_cell::sync::Lazy;

    pub struct ProfileScope {
        name: &'static str,
        start: Instant,
    }

    impl ProfileScope {
        pub fn new(name: &'static str) -> Self {
            ProfileScope {
                name,
                start: Instant::now(),
            }
        }
    }

    impl Drop for ProfileScope {
        fn drop(&mut self) {
            let elapsed = self.start.elapsed();
            let mut lock = PROFILE_DATA.lock().unwrap();
            let data = lock.entry(self.name).or_default();
            data.invocations += 1;
            data.total_time += elapsed;
        }
    }

    static PROFILE_DATA: Lazy<Mutex<HashMap<&'static str, ProfileData>>> =
        Lazy::new(Default::default);

    #[derive(Default)]
    struct ProfileData {
        invocations: u32,
        total_time: Duration,
    }

    pub fn profiling_frame_end(nodes: u64, time: Duration) {
        let report = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open("profile.txt")
            .unwrap();
        let mut report = std::io::BufWriter::new(report);

        let mut data = PROFILE_DATA.lock().unwrap();
        let total_time: Duration = data.values().map(|d| d.total_time).sum();
        writeln!(report, "Total time: {:.2?}", total_time).unwrap();
        writeln!(report, "{:.0} nodes/second", nodes as f64 / time.as_secs_f64());
        for (name, data) in data.drain() {
            writeln!(
                report,
                "{name:20} {spent:.2?} ({percent:.2}%) invocations: {invocations}   avg: {avg:.2?}",
                name = name,
                spent = data.total_time,
                invocations = data.invocations,
                percent = data.total_time.as_secs_f64() / total_time.as_secs_f64() * 100.0,
                avg = data.total_time / data.invocations
            )
            .unwrap();
        }
        writeln!(report).unwrap();
    }
}

#[cfg(not(feature = "profile"))]
mod profile {
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

    pub fn profiling_frame_end(_nodes: u64, _time: std::time::Duration) {}
}
