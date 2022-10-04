use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub token_secret: String,
    pub on_close: Callback<()>,
}

pub enum Msg {
    Close,
}

pub struct AccessTokenCreatedSuccessModal;

impl Component for AccessTokenCreatedSuccessModal {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Close => {
                ctx.props().on_close.emit(());
                BackdropDispatcher::default().close();
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html!(
            <Bullseye plain=true>
                <Modal
                    title="Access token successfully created"
                    variant={ModalVariant::Medium}
                    footer={html!(
                        <Button
                            variant={Variant::Warning}
                            r#type="submit"
                            onclick={ctx.link().callback(|_|Msg::Close)}
                        >
                            {"Close"}
                        </Button>
                    )}
                >
                    <FlexItem>
                    <p>{" The access token value is:"}</p>
                        <p>
                        <Clipboard
                            value={ctx.props().token_secret.clone()}
                            readonly=true
                            name="access-token"
                            />
                        </p>
                        <p>{"Once you close this alert, you won't have any chance to get the access token ever again."}</p>
                         <p>{"Be sure to copy it somewhere safe."}</p>
                    </FlexItem>
                </Modal>
            </Bullseye>
        )
    }
}
