use super::*;

pub struct ApiHandler<T, H>
where
    T: ResponsePayload,
    H: RequestHandler<ApiResponse<T::Target>>,
{
    handler: H,
    _marker: PhantomData<T>,
}

impl<T, H> ApiHandler<T, H>
where
    T: ResponsePayload,
    H: RequestHandler<ApiResponse<T::Target>>,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            _marker: PhantomData::default(),
        }
    }
}

impl<T, H> RequestHandler<anyhow::Result<Response>> for ApiHandler<T, H>
where
    T: ResponsePayload,
    H: RequestHandler<ApiResponse<T::Target>>,
{
    fn execute(
        self,
        context: RequestContext,
        f: impl Future<Output = anyhow::Result<Response>> + 'static,
    ) {
        self.handler.execute(context, async {
            let response = f.await;

            match response {
                Err(err) => ApiResponse::Failure(ApiError::Internal(err)),
                Ok(response) => match convert_json::<T, Json<ErrorInformation>>(
                    response.response.status(),
                    response.data,
                ) {
                    Ok(JsonResponse::Success(code, data)) => ApiResponse::Success(data, code),
                    Ok(JsonResponse::Failure(code, info)) => {
                        ApiResponse::Failure(ApiError::Response(info, code))
                    }
                    Ok(JsonResponse::Invalid(code, _, data)) => {
                        ApiResponse::Failure(ApiError::Unknown(Rc::new(data), code))
                    }
                    Err(err) => ApiResponse::Failure(ApiError::Internal(err)),
                },
            }
        });
    }
}

#[derive(Debug)]
pub enum ApiResponse<T> {
    Success(T, StatusCode),
    Failure(ApiError),
}

#[derive(Debug)]
pub enum ApiError {
    Response(ErrorInformation, StatusCode),
    Unknown(Rc<Vec<u8>>, StatusCode),
    Internal(anyhow::Error),
}
