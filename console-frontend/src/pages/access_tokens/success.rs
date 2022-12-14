use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub token_secret: String,
    pub on_close: Callback<()>,
}

#[function_component(AccessTokenCreatedSuccessModal)]
pub fn access_token_created_success_modal(props: &Props) -> Html {
    let backdropper = use_backdrop().expect("Must have BackdropViewer");

    let close = Callback::from(move |_| backdropper.close());

    html!(
        <Bullseye plain=true>
            <Modal
                title="Success!"
                variant={ModalVariant::Medium}
                footer={html!(
                    <Button
                        variant={Variant::Primary}
                        r#type="submit"
                        onclick={close}
                    >
                        {"Close"}
                    </Button>
                )}
            >
                <FlexItem>
                <p>{"The access token was successfully created. Here is the secret value:"}</p>
                    <br/>
                    <p>
                    <Clipboard
                        value={props.token_secret.clone()}
                        readonly=true
                        name="access-token"
                        />
                    </p>
                    <br/>
                    <Alert
                        inline=true
                        title="You won't be able to see this secret again!" r#type={Type::Warning}
                    >
                        {"Once you close this alert, you won't have any chance to get the access token again as we don't store it."}
                        <p>{"Please make sure to copy it somewhere safe!"}</p>
                    </Alert>
                </FlexItem>
            </Modal>
        </Bullseye>
    )
}
