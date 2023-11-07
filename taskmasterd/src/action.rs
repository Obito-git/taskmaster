use serde::{Deserialize, Serialize};

#[derive(Eq, PartialEq, Serialize, Deserialize, Clone)]
pub enum Action {
    Config(String),
    Maintail(Option<usize>),
    Update(Option<String>),
    Status(Option<String>),
    Start(Option<(String, Option<usize>)>),
    Stop(Option<(String, Option<usize>)>),
    Shutdown,
}