use super::*;

pub trait RequestHandler<T> {
    fn execute(self, context: RequestContext, f: impl Future<Output = T> + 'static);
}

pub struct ComponentHandler<T, COMP, M>
where
    COMP: Component,
    M: FnOnce(T) -> COMP::Message,
{
    link: Scope<COMP>,
    mapper: M,
    _marker: PhantomData<T>,
}

impl<T, COMP, M> ComponentHandler<T, COMP, M>
where
    COMP: Component,
    M: FnOnce(T) -> COMP::Message + 'static,
{
    pub fn new(ctx: &Context<COMP>, mapper: M) -> Self {
        Self {
            link: ctx.link().clone(),
            mapper,
            _marker: PhantomData::default(),
        }
    }
}

impl<T, COMP, M> RequestHandler<T> for ComponentHandler<T, COMP, M>
where
    COMP: Component,
    M: FnOnce(T) -> COMP::Message + 'static,
{
    fn execute(self, context: RequestContext, f: impl Future<Output = T> + 'static) {
        self.link.send_future_batch(async move {
            let result = f.await;

            if context.is_active() {
                vec![(self.mapper)(result)]
            } else {
                vec![]
            }
        });
    }
}

pub trait ComponentHandlerScopeExt<COMP>
where
    COMP: Component,
{
    fn callback_after<T, M>(&self, mapper: M) -> ComponentHandler<T, COMP, M>
    where
        M: FnOnce(T) -> COMP::Message + 'static;
}

impl<COMP> ComponentHandlerScopeExt<COMP> for Context<COMP>
where
    COMP: Component,
{
    fn callback_after<T, M>(&self, mapper: M) -> ComponentHandler<T, COMP, M>
    where
        M: FnOnce(T) -> COMP::Message + 'static,
    {
        ComponentHandler::new(self, mapper)
    }
}

pub struct MappingHandler<T, U, H, M>
where
    H: RequestHandler<U>,
    M: FnOnce(T) -> U,
{
    handler: H,
    mapper: M,
    _marker: PhantomData<(T, U)>,
}

impl<T, U, H, M> MappingHandler<T, U, H, M>
where
    H: RequestHandler<U>,
    M: FnOnce(T) -> U,
{
    #[allow(dead_code)]
    pub fn new(handler: H, mapper: M) -> Self {
        Self {
            handler,
            mapper,
            _marker: PhantomData::default(),
        }
    }
}

impl<T, U, H, M> RequestHandler<T> for MappingHandler<T, U, H, M>
where
    H: RequestHandler<U>,
    M: FnOnce(T) -> U + 'static,
{
    fn execute(self, context: RequestContext, f: impl Future<Output = T> + 'static) {
        self.handler
            .execute(context, async { (self.mapper)(f.await) });
    }
}
