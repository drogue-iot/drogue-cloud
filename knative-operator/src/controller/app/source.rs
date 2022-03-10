use super::{condition_ready, retry, ConstructContext};
use crate::{controller::ControllerConfig, ANNOTATION_APP_NAME, DEFAULT_IMAGE};
use async_trait::async_trait;
use drogue_client::registry::v1::KnativeAppSpec;
use drogue_client::{
    registry::v1::{Application, KafkaAppStatus},
    Translator,
};
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{self, OperationOutcome, ProgressOperation},
    ReconcileError,
};
use drogue_cloud_service_api::kafka::{make_kafka_resource_name, ResourceType};
use k8s_openapi::{api::apps::v1::Deployment, apimachinery::pkg::apis::meta::v1::LabelSelector};
use kube::Api;
use operator_framework::{
    install::container::{ApplyContainer, ApplyEnvironmentVariable},
    process::create_or_update,
    utils::UseOrCreate,
};
use std::collections::BTreeMap;

pub struct CreateDeployment<'o> {
    pub deployments: &'o Api<Deployment>,
    pub config: &'o ControllerConfig,
}

impl CreateDeployment<'_> {
    async fn ensure_deployment(
        &self,
        ctx: &ConstructContext,
        spec: &KnativeAppSpec,
    ) -> Result<Deployment, ReconcileError> {
        let topic_name = make_kafka_resource_name(ResourceType::Events(&ctx.app.metadata.name));

        let deployment = create_or_update(
            self.deployments,
            Some(self.config.target_namespace.clone()),
            &topic_name,
            |deployment| self.reconcile_deployment(deployment, ctx, spec),
        )
        .await
        .map_err(|err| {
            ReconcileError::permanent(format!("Failed to reconcile source deployment: {err}"))
        })?
        .resource();

        // done

        Ok(deployment)
    }

    fn reconcile_deployment(
        &self,
        mut deployment: Deployment,
        ctx: &ConstructContext,
        spec: &KnativeAppSpec,
    ) -> anyhow::Result<Deployment> {
        let selector_labels = BTreeMap::new();

        let mut labels = BTreeMap::new();
        labels.extend(selector_labels.clone());

        // set app name annotation

        deployment
            .metadata
            .annotations
            .use_or_create(|annotations| {
                annotations.insert(
                    ANNOTATION_APP_NAME.to_string(),
                    ctx.app.metadata.name.clone(),
                )
            });

        // reconcile pod spec

        deployment.spec.use_or_create(|spec| {
            spec.selector = LabelSelector {
                match_labels: Some(selector_labels),
                ..Default::default()
            };
            spec.template.metadata.use_or_create(|metadata| {
                metadata.labels = Some(labels.clone());
            });
        });

        // reconcile the container

        deployment.apply_container("source", |mut container| {
            let tpl = &self.config.template;
            let image = tpl.image.clone().unwrap_or_else(|| DEFAULT_IMAGE.into());

            container.image_pull_policy = Some(
                self.config
                    .template
                    .image_pull_policy
                    .clone()
                    .unwrap_or_else(|| {
                        if image.ends_with(":latest") {
                            "Always"
                        } else {
                            "IfNotPresent"
                        }
                        .into()
                    }),
            );
            container.image = Some(image);

            container.args = None;
            container.command = None;
            container.working_dir = None;

            container.add_env("MODE", "kafka")?;

            container.add_env("K_SINK", &spec.endpoint.url)?;

            // FIXME: add from config
            //container.add_env("BOOTSTRAP_SERVERS", self.config.);

            Ok(())
        })?;

        // done

        Ok(deployment)
    }
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreateDeployment<'o> {
    fn type_name(&self) -> String {
        "CreateDeployment".into()
    }

    async fn run(
        &self,
        mut ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let spec = ctx
            .app
            .section::<KnativeAppSpec>()
            .transpose()?
            .ok_or_else(|| {
                ReconcileError::permanent("Missing spec section although it was detected earlier")
            })?;
        let deployment = self.ensure_deployment(&ctx, &spec).await?;

        ctx.deployment = Some(deployment);

        // done

        Ok(OperationOutcome::Continue(ctx))
    }
}

pub struct SourceReady<'o> {
    pub config: &'o ControllerConfig,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for SourceReady<'o> {
    fn type_name(&self) -> String {
        "SourceReady".into()
    }

    async fn run(&self, mut ctx: ConstructContext) -> progress::Result<ConstructContext> {
        let deployment_ready = ctx
            .deployment
            .as_ref()
            .and_then(|deployment| condition_ready("Available", deployment))
            .unwrap_or_default();

        ctx.app.update_section(|mut status: KafkaAppStatus| {
            // using the internal model only for now
            status.downstream = None;
            status
        })?;

        match deployment_ready {
            true => Ok(OperationOutcome::Continue(ctx)),
            false => retry(ctx),
        }
    }
}
