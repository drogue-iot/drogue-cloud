#[derive(Clone, Copy)]
pub struct PagingOptions {
    pub offset: i32,
    pub limit: i32,
}

impl Default for PagingOptions {
    fn default() -> Self {
        PagingOptions {
            offset: 0,
            limit: 20,
        }
    }
}

impl PagingOptions {
    pub fn previous(self) -> Self {
        PagingOptions {
            offset: self.offset - self.limit,
            limit: self.limit,
        }
    }

    pub fn next(self) -> Self {
        PagingOptions {
            offset: self.offset + self.limit,
            limit: self.limit,
        }
    }

    pub fn first(self) -> Self {
        PagingOptions {
            offset: 0,
            limit: self.limit,
        }
    }

    pub fn last(self, max: i32) -> Self {
        PagingOptions {
            offset: max - self.limit,
            limit: self.limit,
        }
    }
}
