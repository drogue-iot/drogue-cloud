#[derive(Clone, Debug)]
pub enum EventTarget {
    Events(String),
    Commands(String),
}

impl EventTarget {
    pub fn app_name(&self) -> &str {
        match self {
            Self::Commands(app) => app.as_str(),
            Self::Events(app) => app.as_str(),
        }
    }
}
