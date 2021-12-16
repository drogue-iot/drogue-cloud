use super::*;

pub struct JsonHandler<T, E, H>
where
    T: for<'de> Deserialize<'de>,
    E: for<'de> Deserialize<'de>,
    H: RequestHandler<anyhow::Result<JsonResponse<T, E>>>,
{
    handler: H,
    _marker: PhantomData<(T, E)>,
}

impl<T, E, H> JsonHandler<T, E, H>
where
    T: for<'de> Deserialize<'de>,
    E: for<'de> Deserialize<'de>,
    H: RequestHandler<anyhow::Result<JsonResponse<T, E>>>,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            _marker: PhantomData::default(),
        }
    }
}

impl<T, E, H> RequestHandler<anyhow::Result<Response>> for JsonHandler<T, E, H>
where
    T: for<'de> Deserialize<'de>,
    E: for<'de> Deserialize<'de>,
    H: RequestHandler<anyhow::Result<JsonResponse<T, E>>>,
{
    fn execute(
        self,
        context: RequestContext,
        f: impl Future<Output = anyhow::Result<Response>> + 'static,
    ) {
        self.handler.execute(context, async {
            let response = f.await;

            match response {
                Err(err) => Err(err),
                Ok(response) => {
                    convert_json::<Json<T>, Json<E>>(response.response.status(), response.data)
                }
            }
        });
    }
}

#[derive(Clone, Debug)]
pub enum JsonResponse<T, E> {
    Success(StatusCode, T),
    Failure(StatusCode, E),
    Invalid(StatusCode, String, Vec<u8>),
}

pub fn convert_json<T, E>(
    status: u16,
    data: Vec<u8>,
) -> anyhow::Result<JsonResponse<T::Target, E::Target>>
where
    T: ResponsePayload,
    E: ResponsePayload,
{
    let status = if let Ok(status) = StatusCode::from_u16(status) {
        status
    } else {
        anyhow::bail!("Invalid status code: {}", status);
    };

    Ok(match status.as_u16() {
        200..=299 => match T::convert_target(&data) {
            Ok(data) => JsonResponse::Success(status, data),
            Err(err) => JsonResponse::Invalid(status, err.to_string(), data),
        },
        _ => match E::convert_target(&data) {
            Ok(data) => JsonResponse::Failure(status, data),
            Err(err) => JsonResponse::Invalid(status, err.to_string(), data),
        },
    })
}

pub trait JsonHandlerScopeExt<COMP>
where
    COMP: Component,
{
    fn callback_json<T, E, M>(
        &self,
        mapper: M,
    ) -> JsonHandler<T, E, ComponentHandler<anyhow::Result<JsonResponse<T, E>>, COMP, M>>
    where
        M: FnOnce(anyhow::Result<JsonResponse<T, E>>) -> COMP::Message + 'static,
        T: for<'de> Deserialize<'de>,
        E: for<'de> Deserialize<'de>;

    fn callback_api<T, M>(
        &self,
        mapper: M,
    ) -> ApiHandler<T, ComponentHandler<ApiResponse<T::Target>, COMP, M>>
    where
        M: FnOnce(ApiResponse<T::Target>) -> COMP::Message + 'static,
        T: ResponsePayload;
}

impl<COMP> JsonHandlerScopeExt<COMP> for Context<COMP>
where
    COMP: Component,
{
    fn callback_json<T, E, M>(
        &self,
        mapper: M,
    ) -> JsonHandler<T, E, ComponentHandler<anyhow::Result<JsonResponse<T, E>>, COMP, M>>
    where
        M: FnOnce(anyhow::Result<JsonResponse<T, E>>) -> COMP::Message + 'static,
        T: for<'de> Deserialize<'de>,
        E: for<'de> Deserialize<'de>,
    {
        JsonHandler::new(ComponentHandler::new(self, mapper))
    }

    fn callback_api<T, M>(
        &self,
        mapper: M,
    ) -> ApiHandler<T, ComponentHandler<ApiResponse<T::Target>, COMP, M>>
    where
        M: FnOnce(ApiResponse<T::Target>) -> COMP::Message + 'static,
        T: ResponsePayload,
    {
        ApiHandler::new(ComponentHandler::new(self, mapper))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_no_content() {
        let result = convert_json::<(), Json<Value>>(StatusCode::NO_CONTENT.as_u16(), vec![]);
        println!("{:?}", result);
        assert!(matches!(
            result,
            Ok(JsonResponse::Success(StatusCode::NO_CONTENT, ()))
        ));
    }

    #[test]
    fn test_ok() {
        let result = convert_json::<Json<Value>, Json<Value>>(StatusCode::OK.as_u16(), "{}".into());
        println!("{:?}", result);
        assert!(matches!(
            result,
            Ok(JsonResponse::Success(StatusCode::OK, Value::Object(..)))
        ));
    }
}
