use core::fmt::{Display, Formatter};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransferOwnership {
    pub new_user: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Members {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    #[serde(default)]
    pub members: IndexMap<String, MemberEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MemberEntry {
    pub role: Role,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Allow everything, including changing members
    Admin,
    /// Allow reading and writing, but not changing members.
    Manager,
    /// Allow reading only.
    Reader,
}

impl Display for Role {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admin => write!(f, "Administrator"),
            Self::Manager => write!(f, "Manager"),
            Self::Reader => write!(f, "Reader"),
        }
    }
}
