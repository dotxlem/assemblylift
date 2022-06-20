use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use crossbeam_channel::bounded;

use tokio::sync::mpsc;
use tracing::{debug, info, instrument};

use kubelet::container::state::prelude::*;
use kubelet::pod::{Handle as PodHandle, PodKey};
use kubelet::state::common::GenericProviderState;
use kubelet::volume::VolumeRef;

use crate::runtime::Runtime;
use crate::ProviderState;

use super::running::Running;
use super::terminated::Terminated;
use super::ContainerState;

fn volume_path_map(
    container: &Container,
    volumes: &HashMap<String, VolumeRef>,
) -> anyhow::Result<HashMap<PathBuf, Option<PathBuf>>> {
    if let Some(volume_mounts) = container.volume_mounts().as_ref() {
        // Check for volumes added during the `Resources` State that were not specified in original ContainerSpec
        // Currently, these are only volumes required by device plugins.
        let container_vol_names: Vec<String> =
            volume_mounts.iter().cloned().map(|vm| vm.name).collect();
        let mut resource_volumes: HashMap<PathBuf, Option<PathBuf>> =
            volumes
                .iter()
                .filter(|(k, _)| !container_vol_names.contains(k))
                .map(|(k, v)| -> anyhow::Result<(PathBuf, Option<PathBuf>)> {
                    match v {
                        VolumeRef::DeviceVolume(host_path_vol, guest_path) => {
                            let host_path =
                                host_path_vol.get_path().map(|p| p.to_owned()).ok_or_else(
                                    || anyhow::anyhow!("Volume {} has not been mounted yet", k),
                                )?;
                            Ok((host_path, Some(guest_path.to_owned())))
                        }
                        v_ref => Err(anyhow::anyhow!(
                            "Volume mounted at path {:?} was not a DeviceVolume",
                            v_ref.get_path()
                        )),
                    }
                })
                .collect::<anyhow::Result<HashMap<PathBuf, Option<PathBuf>>>>()?;

        resource_volumes.extend(
            volume_mounts
                .iter()
                .map(|vm| -> anyhow::Result<(PathBuf, Option<PathBuf>)> {
                    // Check the volume exists first
                    let vol = volumes.get(&vm.name).ok_or_else(|| {
                        anyhow::anyhow!(
                            "no volume with the name of {} found for container {}",
                            vm.name,
                            container.name()
                        )
                    })?;
                    let host_path = vol.get_path().map(|p| p.to_owned()).ok_or_else(|| {
                        anyhow::anyhow!("Volume {} has not been mounted yet", vm.name)
                    })?;
                    let mut guest_path = PathBuf::from(&vm.mount_path);
                    if let Some(sub_path) = &vm.sub_path {
                        guest_path.push(sub_path);
                    }
                    // We can safely assume that this should be valid UTF-8 because it would have
                    // been validated by the k8s API
                    Ok((host_path, Some(guest_path)))
                })
                .collect::<anyhow::Result<HashMap<PathBuf, Option<PathBuf>>>>()?,
        );
        Ok(resource_volumes)
    } else {
        Ok(HashMap::default())
    }
}

/// The container is starting.
#[derive(Default, Debug, TransitionTo)]
#[transition_to(Running, Terminated)]
pub struct Waiting;

#[async_trait::async_trait]
impl State<ContainerState> for Waiting {
    #[instrument(
    level = "info",
    skip(self, shared, state, container),
    fields(pod_name = state.pod.name(), container_name)
    )]
    async fn next(
        self: Box<Self>,
        shared: SharedState<ProviderState>,
        state: &mut ContainerState,
        container: Manifest<Container>,
    ) -> Transition<ContainerState> {
        let container = container.latest();

        tracing::Span::current().record("container_name", &container.name());

        info!("Starting container for pod");

        let (client, log_path) = {
            let provider_state = shared.read().await;
            (provider_state.client(), provider_state.log_path.clone())
        };

        let (module_data, container_volumes, container_envs) = {
            let mut run_context = state.run_context.write().await;
            let module_data = match run_context.modules.remove(container.name()) {
                Some(data) => data,
                None => {
                    return Transition::next(
                        self,
                        Terminated::new(
                            format!(
                                "Pod {} container {} failed load module data from run context.",
                                state.pod.name(),
                                container.name(),
                            ),
                            true,
                        ),
                    );
                }
            };
            let container_volumes = match volume_path_map(&container, &run_context.volumes) {
                Ok(volumes) => volumes,
                Err(e) => {
                    return Transition::next(
                        self,
                        Terminated::new(
                            format!(
                                "Pod {} container {} failed to map volume paths: {:?}",
                                state.pod.name(),
                                container.name(),
                                e
                            ),
                            true,
                        ),
                    )
                }
            };
            (
                module_data,
                container_volumes,
                run_context
                    .env_vars
                    .remove(container.name())
                    .unwrap_or_default(),
            )
        };

        let mut env = kubelet::provider::env_vars(&container, &state.pod, &client).await;
        env.extend(container_envs);
        let args: Vec<String> = container.args().map(|t| t.to_owned()).unwrap_or_default();

        // TODO: ~magic~ number
        let (tx, rx) = bounded(8);

        let name = format!(
            "{}:{}:{}",
            state.pod.namespace(),
            state.pod.name(),
            container.name()
        );

        let runtime = match Runtime::new(
            name,
            module_data,
            shared.read().await.clone().registry_tx,
            env,
            args,
            container_volumes,
            log_path,
            tx,
        )
            .await
        {
            Ok(runtime) => runtime,
            Err(e) => {
                return Transition::next(
                    self,
                    Terminated::new(
                        format!(
                            "Pod {} container {} failed to construct runtime: {:?}",
                            state.pod.name(),
                            container.name(),
                            e
                        ),
                        true,
                    ),
                )
            }
        };
        debug!("Starting container on thread");
        let container_handle = match runtime.start().await {
            Ok(handle) => handle,
            Err(e) => {
                return Transition::next(
                    self,
                    Terminated::new(
                        format!(
                            "Pod {} container {} failed to start: {:?}",
                            state.pod.name(),
                            container.name(),
                            e
                        ),
                        true,
                    ),
                )
            }
        };
        debug!("WASI Runtime started for container");
        let pod_key = PodKey::from(&state.pod);
        {
            let provider_state = shared.write().await;
            let mut handles_writer = provider_state.handles.write().await;
            let pod_handle = handles_writer
                .entry(pod_key)
                .or_insert_with(|| Arc::new(PodHandle::new(HashMap::new(), state.pod.clone())));
            pod_handle
                .insert_container_handle(state.container_key.clone(), container_handle)
                .await;
        }
        Transition::next(self, Running::new(rx))
    }

    async fn status(
        &self,
        _state: &mut ContainerState,
        _container: &Container,
    ) -> anyhow::Result<Status> {
        Ok(Status::waiting("Module is starting."))
    }
}
