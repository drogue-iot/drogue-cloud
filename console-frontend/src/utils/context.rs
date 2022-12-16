use std::cell::{Ref, RefCell};
use std::fmt::Debug;
use std::ops::Deref;
use std::rc::Rc;
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
    context: Rc<RefCell<Option<C>>>,
}

impl<C> Inner<C>
where
    C: Clone + PartialEq + 'static,
{
    fn replace(&self, context: C) {
        self.context.replace(Some(context));
    }
}

impl<C> ContextListener<C>
where
    C: Clone + PartialEq + 'static,
{
    pub fn new<COMP: BaseComponent>(ctx: &Context<COMP>) -> Option<Self> {
        let context = Rc::new(RefCell::new(None));
        let inner = Inner::<C> {
            context: context.clone(),
        };

        ctx.link()
            .context::<C>(Callback::from(move |value| {
                context.deref().replace(Some(value));
            }))
            .map(|(context, handle)| {
                inner.replace(context.clone());
                Self {
                    inner,
                    _handle: handle,
                }
            })
    }

    pub fn unwrap<COMP: BaseComponent>(ctx: &Context<COMP>) -> Self {
        Self::new(ctx).expect("Unable to find context")
    }

    pub fn expect<COMP: BaseComponent>(ctx: &Context<COMP>, msg: &str) -> Self {
        Self::new(ctx).expect(msg)
    }

    pub fn get(&self) -> Ref<C> {
        Ref::map(self.inner.context.deref().borrow(), |c| c.as_ref().unwrap())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MutableContext<T>
where
    T: Clone + Debug + PartialEq,
{
    pub context: T,
    setter: Callback<Box<dyn FnOnce(&mut T)>>,
}

impl<T> MutableContext<T>
where
    T: Clone + Debug + PartialEq,
{
    pub fn new(context: T, setter: Callback<Box<dyn FnOnce(&mut T)>>) -> Self {
        Self { context, setter }
    }

    pub fn update<F>(&self, updater: F)
    where
        F: FnOnce(&mut T) + 'static,
    {
        self.setter.emit(Box::new(updater));
    }

    pub fn apply(&mut self, mutator: Box<dyn FnOnce(&mut T)>) -> bool {
        let old = self.context.clone();
        mutator(&mut self.context);
        self.context != old
    }
}

impl<T> MutableContext<T>
where
    T: Clone + Debug + PartialEq + 'static,
{
    pub fn set(&self, value: T) {
        self.update(|v| *v = value);
    }
}
