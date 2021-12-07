mod policy;
mod thing;

pub use policy::*;
pub use thing::*;

use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityId(pub String, pub String);

impl Display for EntityId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.0, self.1)
    }
}

impl FromStr for EntityId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split(':').collect::<Vec<_>>()[..] {
            [ns, name] => Ok(EntityId(ns.into(), name.into())),
            _ => Err("Invalid policy ID".into()),
        }
    }
}

impl<S1, S2> From<(S1, S2)> for EntityId
where
    S1: Into<String>,
    S2: Into<String>,
{
    fn from(s: (S1, S2)) -> Self {
        Self(s.0.into(), s.1.into())
    }
}
