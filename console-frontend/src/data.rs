use std::fmt::Debug;
use std::{
    collections::HashSet,
    ops::{Deref, DerefMut},
};
use yew::html::Scope;
use yew::{Callback, Component};
use yew_agent::*;

pub struct SharedDataHolder<T>
where
    T: Default + Debug + Clone + PartialEq + 'static,
{
    link: AgentLink<Self>,
    data: T,

    subscribers: HashSet<HandlerId>,
}

pub enum RequestState<T> {
    Get,
    Set(T),
    Update(Box<dyn FnOnce(&mut T) + Send + Sync>),
}

pub enum Response<T> {
    State(T),
}

impl<T> Agent for SharedDataHolder<T>
where
    T: Default + Debug + Clone + PartialEq + 'static,
{
    type Reach = Context<Self>;
    type Message = ();
    type Input = RequestState<T>;
    type Output = Response<T>;

    fn create(link: AgentLink<Self>) -> Self {
        Self {
            link,
            data: T::default(),
            subscribers: HashSet::new(),
        }
    }

    fn update(&mut self, _: Self::Message) {}

    fn connected(&mut self, id: HandlerId) {
        if id.is_respondable() {
            self.subscribers.insert(id);
        }
    }

    fn handle_input(&mut self, msg: Self::Input, id: HandlerId) {
        match msg {
            Self::Input::Get => self
                .link
                .respond(id, Self::Output::State(self.data.clone())),

            Self::Input::Set(data) => {
                if self.data != data {
                    self.data = data;
                    self.notify_all();
                }
            }

            Self::Input::Update(mutator) => {
                let mut new = self.data.clone();
                log::debug!("old data: {:?}", new);
                mutator(&mut new);
                log::debug!("new data: {:?}", new);
                if new != self.data {
                    self.data = new;
                    self.notify_all();
                }
            }
        }
    }

    fn disconnected(&mut self, id: HandlerId) {
        if id.is_respondable() {
            self.subscribers.remove(&id);
        }
    }
}

impl<T> SharedDataHolder<T>
where
    T: Default + Debug + Clone + PartialEq,
{
    pub fn notify_all(&mut self) {
        for sub in self.subscribers.iter() {
            self.link.respond(*sub, Response::State(self.data.clone()));
        }
    }
}

pub struct SharedDataDispatcher<T>(Dispatcher<SharedDataHolder<T>>)
where
    T: Default + Debug + Clone + PartialEq + 'static;

impl<T> SharedDataDispatcher<T>
where
    T: Default + Debug + Clone + PartialEq + 'static,
{
    pub fn new() -> Self {
        Self(SharedDataHolder::dispatcher())
    }
}

impl<T> Default for SharedDataDispatcher<T>
where
    T: Default + Debug + Clone + PartialEq + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Deref for SharedDataDispatcher<T>
where
    T: Default + Debug + Clone + PartialEq + 'static,
{
    type Target = Dispatcher<SharedDataHolder<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for SharedDataDispatcher<T>
where
    T: Default + Debug + Clone + PartialEq + 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct SharedDataBridge<T>(Box<dyn Bridge<SharedDataHolder<T>>>)
where
    T: Default + Debug + Clone + PartialEq + 'static;

impl<T> SharedDataBridge<T>
where
    T: Default + Debug + Clone + PartialEq,
{
    pub fn new(callback: Callback<Response<T>>) -> SharedDataBridge<T> {
        Self(SharedDataHolder::bridge(callback))
    }

    pub fn from<C, F>(link: &Scope<C>, f: F) -> Self
    where
        C: Component,
        F: Fn(T) -> C::Message + 'static,
    {
        let callback = link.batch_callback(move |msg| match msg {
            Response::State(data) => vec![f(data)],
        });
        Self::new(callback)
    }

    pub fn request_state(&mut self) {
        self.0.send(RequestState::Get);
    }
}

impl<T> Deref for SharedDataBridge<T>
where
    T: Default + Debug + Clone + PartialEq,
{
    type Target = Box<dyn Bridge<SharedDataHolder<T>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> DerefMut for SharedDataBridge<T>
where
    T: Default + Debug + Clone + PartialEq,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait SharedDataOps<T>
where
    T: Default + Debug + Clone + PartialEq,
{
    fn set(&mut self, data: T);

    fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut T) + Send + Sync + 'static;
}

impl<T> SharedDataOps<T> for dyn Bridge<SharedDataHolder<T>>
where
    T: Default + Debug + Clone + PartialEq,
{
    fn set(&mut self, data: T) {
        self.send(RequestState::Set(data))
    }

    fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut T) + Send + Sync + 'static,
    {
        self.send(RequestState::Update(Box::new(f)))
    }
}
