use crate::backend::Nothing;
use crate::{
    backend::{ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, RequestHandle},
    error::{error, ErrorNotification, ErrorNotifier},
    pages::access_tokens::success::AccessTokenCreatedSuccessModal,
};
use drogue_cloud_service_api::token::AccessTokenCreated;
use http::Method;
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub on_close: Callback<()>,
}

pub enum Msg {
    Success(AccessTokenCreated),
    Error(ErrorNotification),
    Create,
    Description(String),
}

pub struct AccessTokenCreateModal {
    description: String,

    fetch_task: Option<RequestHandle>,
}

impl Component for AccessTokenCreateModal {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {
            fetch_task: None,
            description: Default::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Error(msg) => {
                BackdropDispatcher::default().close();
                msg.toast();
            }
            Msg::Create => match self.create(ctx, &self.description) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to create", err),
            },
            Msg::Success(token) => {
                BackdropDispatcher::default().open(Backdrop {
                    content: (html! {
                        <AccessTokenCreatedSuccessModal
                            token_secret={token.token}
                            on_close={ctx.props().on_close.clone()}
                            />
                    }),
                });
            }
            Msg::Description(name) => self.description = name,
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! (
            <Bullseye plain=true>
                <Modal
                    title="Create a new access token"
                    variant={ModalVariant::Small}
                    footer={html!(
                        <Button
                            variant={Variant::Primary}
                            disabled={self.fetch_task.is_some()}
                            r#type="submit"
                            onclick={ctx.link().callback(|_|Msg::Create)}
                            form="create-form"
                            id="confirm-create-token"
                        >
                            {"Create"}
                        </Button>
                    )}
                >
                    <Form id="create-form" method="dialog">
                        <FormGroup>
                            <TextInput
                                autofocus=true
                                onchange={ctx.link().callback(Msg::Description)}
                                placeholder="Description (optional)" />
                        </FormGroup>
                    </Form>
                </Modal>
            </Bullseye>
        )
    }
}

impl AccessTokenCreateModal {
    fn create(
        &self,
        ctx: &Context<Self>,
        description: &str,
    ) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::POST,
            "/api/tokens/v1alpha1",
            vec![("description", description)],
            Nothing,
            vec![],
            ctx.callback_api::<Json<AccessTokenCreated>, _>(move |response| match response {
                ApiResponse::Success(token, _) => Msg::Success(token),
                ApiResponse::Failure(err) => Msg::Error(err.notify("Creation failed")),
            }),
        )?)
    }
}
