use enumset::{EnumSet, EnumSetType};
use serde::{Deserialize, Serialize};

use crate::data::{Board, Piece, Placement};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum FrontendMessage {
    Rules,
    Start(Start),
    Play {
        #[serde(rename = "move")]
        mv: Placement,
    },
    NewPiece {
        piece: Piece,
    },
    Suggest,
    Stop,
    Quit,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum BotMessage {
    Info {
        name: &'static str,
        version: &'static str,
        author: &'static str,
        features: &'static [&'static str],
    },
    Ready,
    Suggestion {
        moves: Vec<Placement>,
        move_info: MoveInfo,
    }
}

#[derive(Deserialize)]
pub struct Start {
    pub board: Board,
    pub queue: Vec<Piece>,
    pub hold: Option<Piece>,
    pub combo: u32,
    pub back_to_back: bool,
    #[serde(default)]
    pub randomizer: Randomizer,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum Randomizer {
    SevenBag {
        #[serde(deserialize_with = "collect_enumset")]
        bag_state: EnumSet<Piece>,
    },
    #[serde(other)]
    Unknown,
}

impl Default for Randomizer {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Serialize)]
pub struct MoveInfo {
    pub nodes: u64,
    pub nps: f64,
    pub extra: String,
}

impl From<Vec<[Option<char>; 10]>> for Board {
    fn from(v: Vec<[Option<char>; 10]>) -> Self {
        let mut cols = [0; 10];
        for x in 0..10 {
            for y in 0..40 {
                if v[y][x].is_some() {
                    cols[x] |= 1 << y;
                }
            }
        }
        Board { cols }
    }
}

fn collect_enumset<'de, D, T>(de: D) -> Result<EnumSet<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: EnumSetType + Deserialize<'de>,
{
    Ok(Vec::<T>::deserialize(de)?.into_iter().collect())
}
