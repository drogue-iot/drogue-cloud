pub use drogue_bazaar::auth::UserInformation;
pub use drogue_client::user::v1::UserDetails;

pub trait IsAdmin {
    fn is_admin(&self) -> bool;
}

impl IsAdmin for UserDetails {
    fn is_admin(&self) -> bool {
        self.roles.iter().any(|s| s == "drogue-admin")
    }
}

impl IsAdmin for UserInformation {
    fn is_admin(&self) -> bool {
        match self {
            Self::Authenticated(details) => details.is_admin(),
            Self::Anonymous => false,
        }
    }
}
