use bollard::Docker;
use nu_plugin::{Plugin, PluginCommand};
use nu_protocol::{LabeledError, Value};
use std::sync::Mutex;
use tokio::runtime::Runtime;

use crate::commands::container::{ContainerInspectCommand, ContainerLsCommand};
use crate::commands::detail::{ContainerDiffCommand, ContainerTopCommand, ImageHistoryCommand};
use crate::commands::image::{ImageInspectCommand, ImageLsCommand};
use crate::commands::network::{NetworkInspectCommand, NetworkLsCommand};
use crate::commands::plugin::{PluginInspectCommand, PluginLsCommand};
use crate::commands::system::{SystemInfoCommand, SystemVersionCommand};
use crate::commands::volume::{VolumeInspectCommand, VolumeLsCommand};

pub struct NudePlugin {
    pub rt: Runtime,
    docker: Mutex<Option<Docker>>,
}

impl NudePlugin {
    pub fn new() -> Self {
        Self {
            rt: Runtime::new().expect("failed to create tokio runtime"),
            docker: Mutex::new(None),
        }
    }

    /// Connect to the local Docker daemon, memoizing the handle for reuse.
    ///
    /// `Docker` is cheap to clone (it wraps an `Arc` internally), so each caller
    /// gets its own handle. `connect_with_local_defaults` selects the unix
    /// socket (or the Windows named pipe) and is lazy — it does not reach the
    /// daemon until the first request is made.
    pub fn docker(&self) -> anyhow::Result<Docker> {
        let mut guard = self.docker.lock().unwrap();
        if let Some(d) = guard.as_ref() {
            return Ok(d.clone());
        }
        let d = Docker::connect_with_local_defaults()?;
        *guard = Some(d.clone());
        Ok(d)
    }

    /// Run a command's async body to completion on the runtime, mapping its
    /// `anyhow` error into the `LabeledError` the plugin trait expects. Every
    /// command's `run` is one call to this.
    pub fn block_on_labeled(
        &self,
        fut: impl std::future::Future<Output = anyhow::Result<Value>>,
    ) -> Result<Value, LabeledError> {
        self.rt
            .block_on(fut)
            .map_err(|e| LabeledError::new(e.to_string()))
    }
}

impl Default for NudePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for NudePlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![
            Box::new(ContainerLsCommand),
            Box::new(ContainerInspectCommand),
            Box::new(ContainerDiffCommand),
            Box::new(ContainerTopCommand),
            Box::new(ImageLsCommand),
            Box::new(ImageInspectCommand),
            Box::new(crate::commands::search::ImageSearchCommand),
            Box::new(ImageHistoryCommand),
            Box::new(NetworkLsCommand),
            Box::new(NetworkInspectCommand),
            Box::new(VolumeLsCommand),
            Box::new(VolumeInspectCommand),
            Box::new(PluginLsCommand),
            Box::new(PluginInspectCommand),
            Box::new(SystemInfoCommand),
            Box::new(SystemVersionCommand),
        ]
    }
}
