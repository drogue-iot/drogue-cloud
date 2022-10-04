#[derive(Clone, Copy)]
pub struct PagingOptions {
    pub offset: u32,
    pub limit: u32,
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
            offset: self.offset.saturating_sub(self.limit),
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
    #[allow(dead_code)]
    // this is not used right now as drogue API don't return the total number of entries
    pub fn last(self, max: u32) -> Self {
        PagingOptions {
            offset: max.saturating_sub(self.limit),
            limit: self.limit,
        }
    }

    pub fn page(self, page: u32) -> Self {
        PagingOptions {
            offset: (self.limit * page).saturating_sub(self.limit),
            limit: self.limit,
        }
    }
}
