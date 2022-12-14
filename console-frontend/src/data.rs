use std::fmt::Debug;
use std::ops::Deref;
use yew::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct SharedData<T>
where
    T: Clone + Debug + PartialEq,
{
    pub value: T,
}

impl<T> SharedData<T> {
    pub fn set(&self, value: T) {
        todo!("this isn't working yet");
    }

    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut T),
    {
        todo!("this isn't working yet");
    }
}

impl<T> Deref for SharedData<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        todo!()
    }
}

#[hook]
pub fn use_shared_data<T>() -> Option<SharedData<T>>
where
    T: Clone + Debug + PartialEq,
{
    use_context()
}
