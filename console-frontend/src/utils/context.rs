use std::cell::RefCell;
use std::ops::Deref;
use yew::{BaseComponent, Callback, Context, ContextHandle};

pub struct ContextListener<C>
where
    C: Clone + PartialEq + 'static,
{
    inner: Inner<C>,
    _handle: ContextHandle<C>,
}

#[derive(Default)]
struct Inner<C>
where
    C: Clone + PartialEq + 'static,
{
    context: RefCell<C>,
}

impl<C> Inner<C>
where
    C: Clone + PartialEq + 'static,
{
    fn replace(&self, context: C) {
        self.context.replace(context);
    }

    fn get(&self) -> &C {
        &self.context
    }
}

impl<C> ContextListener<Option<C>>
where
    C: Clone + PartialEq + 'static,
{
    pub fn new<COMP: BaseComponent>(ctx: &Context<COMP>) -> Self {
        let mut inner = Inner::default();
        let (context, handle) = ctx.link().context::<C>(Callback::from(|context| {
            inner.replace(Some(context));
        }));

        inner.context = context.clone();
        Self {
            inner,
            _handle: handle,
        }
    }
}

impl<C> ContextListener<C>
where
    C: Clone + PartialEq + 'static,
{
    pub fn new<COMP: BaseComponent>(ctx: &Context<COMP>) -> Self {
        let mut inner = Inner::default();
        let (context, handle) = ctx.link().context::<C>(Callback::from(|context| {
            inner.replace(context);
        }));

        inner.context = context.clone();
        Self {
            inner,
            _handle: handle,
        }
    }
}

impl<C> Deref for ContextListener<C>
where
    C: Clone + PartialEq + 'static,
{
    type Target = C;

    fn deref(&self) -> &Self::Target {
        self.inner.get()
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test() {
        let inner = Inner::<Option<String>>::default();
        assert_eq!(inner.get(), &None);
        inner.replace(Some("foo".to_string()));
        assert_eq!(inner.get(), &Some("foo".to_string()));
    }
}
