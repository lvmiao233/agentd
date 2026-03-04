use agentd_core::AgentError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixListener;
use tokio::process::{Child, Command};
use tokio::time::{timeout, Duration};
use uuid::Uuid;

const DEFAULT_VSOCK_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirecrackerNetworkConfig {
    pub tap_device: String,
    pub host_ipv4: String,
    pub guest_ipv4: String,
}

impl Default for FirecrackerNetworkConfig {
    fn default() -> Self {
        Self {
            tap_device: "fc-tap0".to_string(),
            host_ipv4: "172.16.0.1/30".to_string(),
            guest_ipv4: "172.16.0.2/30".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirecrackerVmConfig {
    pub kernel_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub vcpu_count: u8,
    pub mem_size_mib: u32,
    pub network: Option<FirecrackerNetworkConfig>,
    pub vsock_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FirecrackerAgentLaunchSpec {
    pub agent_id: Uuid,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub vcpu_count: Option<u8>,
    pub mem_size_mib: Option<u32>,
    pub network: Option<FirecrackerNetworkConfig>,
    pub launch_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct FirecrackerExecutor {
    kernel_path: PathBuf,
    rootfs_path: PathBuf,
    default_vcpu_count: u8,
    default_mem_size_mib: u32,
    default_network: Option<FirecrackerNetworkConfig>,
    vsock_root_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FirecrackerExecutorBuilder {
    kernel_path: Option<PathBuf>,
    rootfs_path: Option<PathBuf>,
    default_vcpu_count: u8,
    default_mem_size_mib: u32,
    default_network: Option<FirecrackerNetworkConfig>,
    vsock_root_dir: PathBuf,
}

impl Default for FirecrackerExecutorBuilder {
    fn default() -> Self {
        Self {
            kernel_path: None,
            rootfs_path: None,
            default_vcpu_count: 1,
            default_mem_size_mib: 512,
            default_network: Some(FirecrackerNetworkConfig::default()),
            vsock_root_dir: std::env::temp_dir().join("agentd-firecracker"),
        }
    }
}

impl FirecrackerExecutorBuilder {
    pub fn kernel_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.kernel_path = Some(path.into());
        self
    }

    pub fn rootfs_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.rootfs_path = Some(path.into());
        self
    }

    pub fn default_vcpu_count(mut self, vcpu_count: u8) -> Self {
        self.default_vcpu_count = vcpu_count;
        self
    }

    pub fn default_mem_size_mib(mut self, mem_size_mib: u32) -> Self {
        self.default_mem_size_mib = mem_size_mib;
        self
    }

    pub fn vsock_root_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.vsock_root_dir = path.into();
        self
    }

    pub fn build(self) -> Result<FirecrackerExecutor, AgentError> {
        let kernel_path = self
            .kernel_path
            .ok_or_else(|| AgentError::InvalidInput("firecracker kernel_path is required".to_string()))?;
        let rootfs_path = self
            .rootfs_path
            .ok_or_else(|| AgentError::InvalidInput("firecracker rootfs_path is required".to_string()))?;

        if self.default_vcpu_count == 0 {
            return Err(AgentError::InvalidInput(
                "firecracker vcpu_count must be greater than zero".to_string(),
            ));
        }

        if self.default_mem_size_mib == 0 {
            return Err(AgentError::InvalidInput(
                "firecracker mem_size_mib must be greater than zero".to_string(),
            ));
        }

        Ok(FirecrackerExecutor {
            kernel_path,
            rootfs_path,
            default_vcpu_count: self.default_vcpu_count,
            default_mem_size_mib: self.default_mem_size_mib,
            default_network: self.default_network,
            vsock_root_dir: self.vsock_root_dir,
        })
    }
}

impl FirecrackerExecutor {
    pub fn builder() -> FirecrackerExecutorBuilder {
        FirecrackerExecutorBuilder::default()
    }

    pub fn vsock_path_for_agent(&self, agent_id: Uuid) -> PathBuf {
        let compact = agent_id.simple().to_string();
        let suffix = &compact[..12];
        self.vsock_root_dir.join(format!("a-{suffix}.sock"))
    }

    pub fn build_vm_config(
        &self,
        spec: &FirecrackerAgentLaunchSpec,
    ) -> Result<FirecrackerVmConfig, AgentError> {
        ensure_path_exists(&self.kernel_path, "kernel")?;
        ensure_path_exists(&self.rootfs_path, "rootfs")?;

        let vcpu_count = spec.vcpu_count.unwrap_or(self.default_vcpu_count);
        if vcpu_count == 0 {
            return Err(AgentError::InvalidInput(
                "firecracker launch vcpu_count must be greater than zero".to_string(),
            ));
        }

        let mem_size_mib = spec.mem_size_mib.unwrap_or(self.default_mem_size_mib);
        if mem_size_mib == 0 {
            return Err(AgentError::InvalidInput(
                "firecracker launch mem_size_mib must be greater than zero".to_string(),
            ));
        }

        Ok(FirecrackerVmConfig {
            kernel_path: self.kernel_path.clone(),
            rootfs_path: self.rootfs_path.clone(),
            vcpu_count,
            mem_size_mib,
            network: spec
                .network
                .clone()
                .or_else(|| self.default_network.clone()),
            vsock_path: self.vsock_path_for_agent(spec.agent_id),
        })
    }

    pub async fn launch_agent(
        &self,
        spec: FirecrackerAgentLaunchSpec,
    ) -> Result<FirecrackerVmHandle, AgentError> {
        if spec.command.trim().is_empty() {
            return Err(AgentError::InvalidInput(
                "firecracker launch command must be non-empty".to_string(),
            ));
        }

        let vm_config = self.build_vm_config(&spec)?;
        prepare_vsock_socket(&vm_config.vsock_path)?;

        let listener = UnixListener::bind(&vm_config.vsock_path).map_err(|err| {
            AgentError::Runtime(format!(
                "bind firecracker vsock bridge failed: path={} error={err}",
                vm_config.vsock_path.display()
            ))
        })?;

        let mut command = Command::new(&spec.command);
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .env("AGENTD_VSOCK_PATH", &vm_config.vsock_path)
            .env("AGENTD_FIRECRACKER_KERNEL", &vm_config.kernel_path)
            .env("AGENTD_FIRECRACKER_ROOTFS", &vm_config.rootfs_path)
            .env(
                "AGENTD_FIRECRACKER_VCPU_COUNT",
                vm_config.vcpu_count.to_string(),
            )
            .env(
                "AGENTD_FIRECRACKER_MEM_MIB",
                vm_config.mem_size_mib.to_string(),
            );
        if let Some(network) = &vm_config.network {
            command
                .env("AGENTD_FIRECRACKER_TAP", &network.tap_device)
                .env("AGENTD_FIRECRACKER_HOST_IPV4", &network.host_ipv4)
                .env("AGENTD_FIRECRACKER_GUEST_IPV4", &network.guest_ipv4);
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = cleanup_vsock_socket(&vm_config.vsock_path);
                return Err(AgentError::Runtime(format!(
                    "spawn firecracker vm process failed: command={} error={err}",
                    spec.command
                )));
            }
        };

        let accept_result = timeout(spec.launch_timeout, listener.accept()).await;
        let (stream, _) = match accept_result {
            Ok(Ok(accepted)) => accepted,
            Ok(Err(err)) => {
                let _ = terminate_child(&mut child).await;
                let _ = cleanup_vsock_socket(&vm_config.vsock_path);
                return Err(AgentError::Runtime(format!(
                    "accept firecracker vsock bridge failed: path={} error={err}",
                    vm_config.vsock_path.display()
                )));
            }
            Err(_) => {
                let _ = terminate_child(&mut child).await;
                let _ = cleanup_vsock_socket(&vm_config.vsock_path);
                return Err(AgentError::Runtime(format!(
                    "firecracker launch timeout: agent_id={} timeout_ms={}",
                    spec.agent_id,
                    spec.launch_timeout.as_millis()
                )));
            }
        };

        let (read_half, write_half) = stream.into_split();
        Ok(FirecrackerVmHandle {
            agent_id: spec.agent_id,
            config: vm_config,
            child,
            read_half: BufReader::new(read_half),
            write_half,
        })
    }
}

#[derive(Debug)]
pub struct FirecrackerVmHandle {
    pub agent_id: Uuid,
    config: FirecrackerVmConfig,
    child: Child,
    read_half: BufReader<OwnedReadHalf>,
    write_half: OwnedWriteHalf,
}

impl FirecrackerVmHandle {
    pub fn agent_id(&self) -> Uuid {
        self.agent_id
    }

    pub fn config(&self) -> &FirecrackerVmConfig {
        &self.config
    }

    pub async fn roundtrip_json(&mut self, request: &Value) -> Result<Value, AgentError> {
        let encoded = serde_json::to_string(request).map_err(|err| {
            AgentError::Protocol(format!("encode firecracker vsock request failed: {err}"))
        })?;
        self.write_half
            .write_all(encoded.as_bytes())
            .await
            .map_err(|err| {
                AgentError::Runtime(format!("write firecracker vsock request failed: {err}"))
            })?;
        self.write_half.write_all(b"\n").await.map_err(|err| {
            AgentError::Runtime(format!("write firecracker vsock newline failed: {err}"))
        })?;
        self.write_half.flush().await.map_err(|err| {
            AgentError::Runtime(format!("flush firecracker vsock request failed: {err}"))
        })?;

        let mut line = String::new();
        let read = timeout(
            Duration::from_secs(DEFAULT_VSOCK_TIMEOUT_SECS),
            self.read_half.read_line(&mut line),
        )
        .await
        .map_err(|_| {
            AgentError::Runtime(format!(
                "firecracker vsock roundtrip timeout after {}s",
                DEFAULT_VSOCK_TIMEOUT_SECS
            ))
        })?
        .map_err(|err| AgentError::Runtime(format!("read firecracker vsock response failed: {err}")))?;

        if read == 0 {
            return Err(AgentError::Runtime(
                "firecracker vsock peer closed connection".to_string(),
            ));
        }

        serde_json::from_str::<Value>(line.trim_end()).map_err(|err| {
            AgentError::Protocol(format!("decode firecracker vsock response failed: {err}"))
        })
    }

    pub async fn shutdown(mut self) -> Result<(), AgentError> {
        terminate_child(&mut self.child).await?;
        cleanup_vsock_socket(&self.config.vsock_path)
    }
}

impl Drop for FirecrackerVmHandle {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = cleanup_vsock_socket(&self.config.vsock_path);
    }
}

fn ensure_path_exists(path: &Path, label: &str) -> Result<(), AgentError> {
    if path.exists() {
        Ok(())
    } else {
        Err(AgentError::Config(format!(
            "firecracker {} path does not exist: {}",
            label,
            path.display()
        )))
    }
}

fn prepare_vsock_socket(path: &Path) -> Result<(), AgentError> {
    let parent = path.parent().ok_or_else(|| {
        AgentError::Runtime(format!(
            "firecracker vsock path has no parent: {}",
            path.display()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|err| {
        AgentError::Runtime(format!(
            "create firecracker vsock parent failed: path={} error={err}",
            parent.display()
        ))
    })?;

    cleanup_vsock_socket(path)
}

fn cleanup_vsock_socket(path: &Path) -> Result<(), AgentError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AgentError::Runtime(format!(
            "cleanup firecracker vsock socket failed: path={} error={err}",
            path.display()
        ))),
    }
}

async fn terminate_child(child: &mut Child) -> Result<(), AgentError> {
    match child.try_wait() {
        Ok(Some(_)) => return Ok(()),
        Ok(None) => {}
        Err(err) => {
            return Err(AgentError::Runtime(format!(
                "query firecracker vm process status failed: {err}"
            )))
        }
    }

    child.start_kill().map_err(|err| {
        AgentError::Runtime(format!("signal firecracker vm process kill failed: {err}"))
    })?;
    child.wait().await.map_err(|err| {
        AgentError::Runtime(format!("wait firecracker vm process exit failed: {err}"))
    })?;

    Ok(())
}
