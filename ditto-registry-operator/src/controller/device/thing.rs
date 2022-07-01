use super::{ConstructContext, DeconstructContext};
use crate::{
    controller::{policy_id, ControllerConfig},
    ditto::{
        self,
        api::ThingOperation,
        data::{EntityId, Thing},
    },
};
use async_trait::async_trait;
use drogue_client::{
    openid::OpenIdTokenProvider,
    registry::v1::{Application, Device},
};
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{OperationOutcome, ProgressOperation},
    ReconcileError,
};
use http::StatusCode;
use tracing::instrument;

pub struct CreateThing<'o> {
    pub config: &'o ControllerConfig,
    pub ditto: &'o ditto::Client,
    pub provider: &'o OpenIdTokenProvider,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreateThing<'o> {
    fn type_name(&self) -> String {
        "CreateThing".into()
    }

    #[instrument(skip_all, ret)]
    async fn run(
        &self,
        ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let thing_id = thing_id(&ctx.app, &ctx.device);

        let resp = self
            .ditto
            .request(
                self.provider,
                ThingOperation::CreateOrUpdate(Box::new(Thing {
                    thing_id,
                    policy_id: policy_id(&ctx.app),
                    definition: None,
                    attributes: Default::default(),
                    features: Default::default(),
                })),
            )
            .await
            .map_err(map_ditto_error)?;

        log::debug!("Response: {:#?}", resp);

        // Done
        Ok(OperationOutcome::Continue(ctx))
    }
}

pub struct DeleteThing<'o> {
    pub config: &'o ControllerConfig,
    pub ditto: &'o ditto::Client,
    pub provider: &'o OpenIdTokenProvider,
}

impl<'o> DeleteThing<'o> {
    #[instrument(skip_all, ret)]
    pub async fn run(&self, ctx: &DeconstructContext) -> Result<(), ReconcileError> {
        let resp = self
            .ditto
            .request(
                self.provider,
                ThingOperation::Delete(thing_id(&ctx.app, &ctx.device)),
            )
            .await;

        if let Err(ditto::Error::Response(StatusCode::NOT_FOUND)) = resp {
            // already gone
            log::debug!("Thing was already gone");
            return Ok(());
        }

        let resp = resp.map_err(map_ditto_error)?;
        log::debug!("Response: {:#?}", resp);

        Ok(())
    }
}

fn thing_id(app: &Application, device: &Device) -> EntityId {
    EntityId(app.metadata.name.clone(), device.metadata.name.clone())
}

fn map_ditto_error(err: ditto::Error) -> ReconcileError {
    match err.is_temporary() {
        true => ReconcileError::temporary(err),
        false => ReconcileError::permanent(err),
    }
}
