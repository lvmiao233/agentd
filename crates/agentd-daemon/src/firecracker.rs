use agentd_core::AgentError;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
#[cfg(test)]
use tokio::io::AsyncBufReadExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout, Duration, Instant};
use uuid::Uuid;

#[cfg(test)]
const DEFAULT_VSOCK_TIMEOUT_SECS: u64 = 5;

const API_POLL_INTERVAL_MS: u64 = 25;
const READY_PROBE_PAYLOAD: &str = r#"{"rpc":"daemon.ready"}"#;
const DEFAULT_FIRECRACKER_BINARY: &str = "/usr/bin/firecracker";
const DEFAULT_FIRECRACKER_BOOT_ARGS: &str = "console=ttyS0 reboot=k panic=1 pci=off";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirecrackerLaunchMode {
    RealFirecracker,
    MockProcess,
}

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
#[serde(rename_all = "snake_case")]
pub enum NetworkIsolationPolicy {
    AllowAll,
    DenyAll,
}

impl Default for NetworkIsolationPolicy {
    fn default() -> Self {
        Self::AllowAll
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JailerConfig {
    pub uid: u32,
    pub gid: u32,
    pub seccomp_level: u8,
    pub chroot_base_dir: PathBuf,
    pub netns_path: Option<PathBuf>,
}

impl Default for JailerConfig {
    fn default() -> Self {
        Self {
            uid: 1000,
            gid: 1000,
            seccomp_level: 2,
            chroot_base_dir: PathBuf::from("/run/agentd/jailer"),
            netns_path: None,
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
    pub network_policy: NetworkIsolationPolicy,
    pub jailer: Option<JailerConfig>,
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
    pub network_policy: Option<NetworkIsolationPolicy>,
    pub jailer: Option<JailerConfig>,
    pub launch_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct FirecrackerExecutor {
    firecracker_binary: PathBuf,
    kernel_path: PathBuf,
    rootfs_path: PathBuf,
    default_vcpu_count: u8,
    default_mem_size_mib: u32,
    default_network: Option<FirecrackerNetworkConfig>,
    default_network_policy: NetworkIsolationPolicy,
    default_jailer: Option<JailerConfig>,
    vsock_root_dir: PathBuf,
    api_socket_root_dir: PathBuf,
    launch_mode: FirecrackerLaunchMode,
}

#[derive(Debug, Clone)]
pub struct FirecrackerExecutorBuilder {
    firecracker_binary: PathBuf,
    kernel_path: Option<PathBuf>,
    rootfs_path: Option<PathBuf>,
    default_vcpu_count: u8,
    default_mem_size_mib: u32,
    default_network: Option<FirecrackerNetworkConfig>,
    default_network_policy: NetworkIsolationPolicy,
    default_jailer: Option<JailerConfig>,
    vsock_root_dir: PathBuf,
    api_socket_root_dir: PathBuf,
    launch_mode: FirecrackerLaunchMode,
}

impl Default for FirecrackerExecutorBuilder {
    fn default() -> Self {
        Self {
            firecracker_binary: PathBuf::from(DEFAULT_FIRECRACKER_BINARY),
            kernel_path: None,
            rootfs_path: None,
            default_vcpu_count: 1,
            default_mem_size_mib: 512,
            default_network: Some(FirecrackerNetworkConfig::default()),
            default_network_policy: NetworkIsolationPolicy::AllowAll,
            default_jailer: Some(JailerConfig::default()),
            vsock_root_dir: std::env::temp_dir().join("agentd-firecracker-vsock"),
            api_socket_root_dir: std::env::temp_dir().join("agentd-firecracker-api"),
            launch_mode: {
                #[cfg(test)]
                {
                    FirecrackerLaunchMode::MockProcess
                }
                #[cfg(not(test))]
                {
                    FirecrackerLaunchMode::RealFirecracker
                }
            },
        }
    }
}

impl FirecrackerExecutorBuilder {
    pub fn firecracker_binary(mut self, path: impl Into<PathBuf>) -> Self {
        self.firecracker_binary = path.into();
        self
    }

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

    pub fn api_socket_root_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.api_socket_root_dir = path.into();
        self
    }

    pub fn launch_mode(mut self, mode: FirecrackerLaunchMode) -> Self {
        self.launch_mode = mode;
        self
    }

    pub fn default_network_policy(mut self, policy: NetworkIsolationPolicy) -> Self {
        self.default_network_policy = policy;
        self
    }

    pub fn default_jailer(mut self, jailer: Option<JailerConfig>) -> Self {
        self.default_jailer = jailer;
        self
    }

    pub fn build(self) -> Result<FirecrackerExecutor, AgentError> {
        let kernel_path = self.kernel_path.ok_or_else(|| {
            AgentError::InvalidInput("firecracker kernel_path is required".to_string())
        })?;
        let rootfs_path = self.rootfs_path.ok_or_else(|| {
            AgentError::InvalidInput("firecracker rootfs_path is required".to_string())
        })?;

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
            firecracker_binary: self.firecracker_binary,
            kernel_path,
            rootfs_path,
            default_vcpu_count: self.default_vcpu_count,
            default_mem_size_mib: self.default_mem_size_mib,
            default_network: self.default_network,
            default_network_policy: self.default_network_policy,
            default_jailer: self.default_jailer,
            vsock_root_dir: self.vsock_root_dir,
            api_socket_root_dir: self.api_socket_root_dir,
            launch_mode: self.launch_mode,
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

    pub fn api_socket_for_agent(&self, agent_id: Uuid) -> PathBuf {
        let compact = agent_id.simple().to_string();
        let suffix = &compact[..12];
        self.api_socket_root_dir.join(format!("a-{suffix}.api.sock"))
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
            network_policy: spec
                .network_policy
                .clone()
                .unwrap_or_else(|| self.default_network_policy.clone()),
            jailer: spec.jailer.clone().or_else(|| self.default_jailer.clone()),
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
        enforce_network_isolation_policy(&vm_config)?;

        match self.launch_mode {
            FirecrackerLaunchMode::MockProcess => self.launch_mock_agent(spec, vm_config).await,
            FirecrackerLaunchMode::RealFirecracker => self.launch_real_vm(spec, vm_config).await,
        }
    }

    async fn launch_mock_agent(
        &self,
        spec: FirecrackerAgentLaunchSpec,
        vm_config: FirecrackerVmConfig,
    ) -> Result<FirecrackerVmHandle, AgentError> {
        prepare_socket_file(&vm_config.vsock_path, "firecracker vsock")?;

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
            )
            .env(
                "AGENTD_FIRECRACKER_NETWORK_POLICY",
                match vm_config.network_policy {
                    NetworkIsolationPolicy::AllowAll => "allow_all",
                    NetworkIsolationPolicy::DenyAll => "deny_all",
                },
            );
        if let Some(network) = &vm_config.network {
            command
                .env("AGENTD_FIRECRACKER_TAP", &network.tap_device)
                .env("AGENTD_FIRECRACKER_HOST_IPV4", &network.host_ipv4)
                .env("AGENTD_FIRECRACKER_GUEST_IPV4", &network.guest_ipv4);
        }
        if let Some(jailer) = &vm_config.jailer {
            command
                .env("AGENTD_JAILER_UID", jailer.uid.to_string())
                .env("AGENTD_JAILER_GID", jailer.gid.to_string())
                .env("AGENTD_JAILER_SECCOMP", jailer.seccomp_level.to_string())
                .env(
                    "AGENTD_JAILER_CHROOT_BASE",
                    jailer.chroot_base_dir.to_string_lossy().to_string(),
                );
            if let Some(netns_path) = &jailer.netns_path {
                command.env(
                    "AGENTD_JAILER_NETNS",
                    netns_path.to_string_lossy().to_string(),
                );
            }
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = cleanup_socket_file(&vm_config.vsock_path, "firecracker vsock");
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
                let _ = cleanup_socket_file(&vm_config.vsock_path, "firecracker vsock");
                return Err(AgentError::Runtime(format!(
                    "accept firecracker vsock bridge failed: path={} error={err}",
                    vm_config.vsock_path.display()
                )));
            }
            Err(_) => {
                let _ = terminate_child(&mut child).await;
                let _ = cleanup_socket_file(&vm_config.vsock_path, "firecracker vsock");
                return Err(AgentError::Runtime(format!(
                    "firecracker launch timeout: agent_id={} timeout_ms={}",
                    spec.agent_id,
                    spec.launch_timeout.as_millis()
                )));
            }
        };

        let (read_half, write_half) = stream.into_split();
        let mut read_half = BufReader::new(read_half);
        let mut write_half = write_half;
        if let Err(err) = probe_guest_readiness(&mut read_half, &mut write_half, spec.launch_timeout).await {
            let _ = terminate_child(&mut child).await;
            let _ = cleanup_socket_file(&vm_config.vsock_path, "firecracker vsock");
            return Err(err);
        }

        Ok(FirecrackerVmHandle {
            agent_id: spec.agent_id,
            config: vm_config,
            child,
            api_socket_path: None,
            transport: VmTransport::Mock {
                read_half,
                write_half,
            },
        })
    }

    async fn launch_real_vm(
        &self,
        spec: FirecrackerAgentLaunchSpec,
        vm_config: FirecrackerVmConfig,
    ) -> Result<FirecrackerVmHandle, AgentError> {
        ensure_path_exists(&self.firecracker_binary, "binary")?;
        prepare_socket_file(&vm_config.vsock_path, "firecracker vsock")?;

        let api_socket = self.api_socket_for_agent(spec.agent_id);
        prepare_socket_file(&api_socket, "firecracker api")?;

        let mut child = match Command::new(&self.firecracker_binary)
            .arg("--api-sock")
            .arg(&api_socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                let _ = cleanup_socket_file(&vm_config.vsock_path, "firecracker vsock");
                let _ = cleanup_socket_file(&api_socket, "firecracker api");
                return Err(AgentError::Runtime(format!(
                    "spawn firecracker binary failed: binary={} error={err}",
                    self.firecracker_binary.display()
                )));
            }
        };

        if let Err(err) = wait_for_unix_socket(&api_socket, spec.launch_timeout).await {
            let _ = terminate_child(&mut child).await;
            let _ = cleanup_socket_file(&vm_config.vsock_path, "firecracker vsock");
            let _ = cleanup_socket_file(&api_socket, "firecracker api");
            return Err(err);
        }

        let guest_cid = guest_cid_for_agent(spec.agent_id);

        let configure = async {
            firecracker_put_json(
                &api_socket,
                "/boot-source",
                &json!({
                    "kernel_image_path": vm_config.kernel_path,
                    "boot_args": DEFAULT_FIRECRACKER_BOOT_ARGS,
                }),
            )
            .await?;

            firecracker_put_json(
                &api_socket,
                "/drives/rootfs",
                &json!({
                    "drive_id": "rootfs",
                    "path_on_host": vm_config.rootfs_path,
                    "is_root_device": true,
                    "is_read_only": false,
                }),
            )
            .await?;

            firecracker_put_json(
                &api_socket,
                "/machine-config",
                &json!({
                    "vcpu_count": vm_config.vcpu_count,
                    "mem_size_mib": vm_config.mem_size_mib,
                }),
            )
            .await?;

            firecracker_put_json(
                &api_socket,
                "/vsock",
                &json!({
                    "vsock_id": "agentd-vsock",
                    "guest_cid": guest_cid,
                    "uds_path": vm_config.vsock_path,
                }),
            )
            .await?;

            if let Some(network) = &vm_config.network {
                firecracker_put_json(
                    &api_socket,
                    "/network-interfaces/eth0",
                    &json!({
                        "iface_id": "eth0",
                        "host_dev_name": network.tap_device,
                        "guest_mac": "06:00:ac:10:00:02",
                    }),
                )
                .await?;
            }

            firecracker_put_json(
                &api_socket,
                "/actions",
                &json!({"action_type": "InstanceStart"}),
            )
            .await
        };

        if let Err(err) = configure.await {
            let _ = terminate_child(&mut child).await;
            let _ = cleanup_socket_file(&vm_config.vsock_path, "firecracker vsock");
            let _ = cleanup_socket_file(&api_socket, "firecracker api");
            return Err(AgentError::Runtime(format!(
                "configure firecracker vm failed: agent_id={} error={err}",
                spec.agent_id
            )));
        }

        wait_for_vm_running(&api_socket, spec.launch_timeout).await?;

        wait_for_path_exists(
            &vm_config.vsock_path,
            spec.launch_timeout,
            "firecracker vsock device",
        )
        .await?;

        Ok(FirecrackerVmHandle {
            agent_id: spec.agent_id,
            config: vm_config,
            child,
            api_socket_path: Some(api_socket),
            transport: VmTransport::Firecracker,
        })
    }
}

#[derive(Debug)]
pub struct FirecrackerVmHandle {
    pub agent_id: Uuid,
    config: FirecrackerVmConfig,
    child: Child,
    api_socket_path: Option<PathBuf>,
    transport: VmTransport,
}

#[derive(Debug)]
enum VmTransport {
    Mock {
        read_half: BufReader<OwnedReadHalf>,
        write_half: OwnedWriteHalf,
    },
    Firecracker,
}

impl FirecrackerVmHandle {
    #[cfg(test)]
    pub fn agent_id(&self) -> Uuid {
        self.agent_id
    }

    #[cfg(test)]
    pub fn config(&self) -> &FirecrackerVmConfig {
        &self.config
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    #[cfg(test)]
    pub async fn roundtrip_json(&mut self, request: &Value) -> Result<Value, AgentError> {
        let VmTransport::Mock {
            read_half,
            write_half,
        } = &mut self.transport
        else {
            return Err(AgentError::Runtime(
                "firecracker vsock roundtrip_json is only available in mock mode".to_string(),
            ));
        };

        let encoded = serde_json::to_string(request).map_err(|err| {
            AgentError::Protocol(format!("encode firecracker vsock request failed: {err}"))
        })?;
        write_half
            .write_all(encoded.as_bytes())
            .await
            .map_err(|err| {
                AgentError::Runtime(format!("write firecracker vsock request failed: {err}"))
            })?;
        write_half.write_all(b"\n").await.map_err(|err| {
            AgentError::Runtime(format!("write firecracker vsock newline failed: {err}"))
        })?;
        write_half.flush().await.map_err(|err| {
            AgentError::Runtime(format!("flush firecracker vsock request failed: {err}"))
        })?;

        let mut line = String::new();
        let read = timeout(
            Duration::from_secs(DEFAULT_VSOCK_TIMEOUT_SECS),
            read_half.read_line(&mut line),
        )
        .await
        .map_err(|_| {
            AgentError::Runtime(format!(
                "firecracker vsock roundtrip timeout after {}s",
                DEFAULT_VSOCK_TIMEOUT_SECS
            ))
        })?
        .map_err(|err| {
            AgentError::Runtime(format!("read firecracker vsock response failed: {err}"))
        })?;

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
        if let VmTransport::Mock { write_half, .. } = &mut self.transport {
            let _ = write_half.shutdown().await;
        }

        if let Some(api_socket) = &self.api_socket_path {
            let _ = firecracker_put_json(
                api_socket,
                "/actions",
                &json!({"action_type": "SendCtrlAltDel"}),
            )
            .await;
        }

        terminate_child(&mut self.child).await?;
        cleanup_socket_file(&self.config.vsock_path, "firecracker vsock")?;
        if let Some(api_socket) = &self.api_socket_path {
            cleanup_socket_file(api_socket, "firecracker api")?;
        }
        Ok(())
    }
}

impl Drop for FirecrackerVmHandle {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = cleanup_socket_file(&self.config.vsock_path, "firecracker vsock");
        if let Some(api_socket) = &self.api_socket_path {
            let _ = cleanup_socket_file(api_socket, "firecracker api");
        }
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

fn prepare_socket_file(path: &Path, label: &str) -> Result<(), AgentError> {
    let parent = path.parent().ok_or_else(|| {
        AgentError::Runtime(format!(
            "{label} path has no parent: {}",
            path.display()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|err| {
        AgentError::Runtime(format!(
            "create {label} parent failed: path={} error={err}",
            parent.display()
        ))
    })?;
    cleanup_socket_file(path, label)
}

fn cleanup_socket_file(path: &Path, label: &str) -> Result<(), AgentError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AgentError::Runtime(format!(
            "cleanup {label} socket failed: path={} error={err}",
            path.display()
        ))),
    }
}

async fn wait_for_unix_socket(path: &Path, timeout_duration: Duration) -> Result<(), AgentError> {
    let deadline = Instant::now() + timeout_duration;
    loop {
        if path.exists() {
            if UnixStream::connect(path).await.is_ok() {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(AgentError::Runtime(format!(
                "firecracker launch timeout waiting for api socket: path={} timeout_ms={}",
                path.display(),
                timeout_duration.as_millis()
            )));
        }
        sleep(Duration::from_millis(API_POLL_INTERVAL_MS)).await;
    }
}

async fn wait_for_path_exists(
    path: &Path,
    timeout_duration: Duration,
    label: &str,
) -> Result<(), AgentError> {
    let deadline = Instant::now() + timeout_duration;
    loop {
        if path.exists() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(AgentError::Runtime(format!(
                "firecracker launch timeout waiting for {label}: path={} timeout_ms={}",
                path.display(),
                timeout_duration.as_millis()
            )));
        }
        sleep(Duration::from_millis(API_POLL_INTERVAL_MS)).await;
    }
}

async fn wait_for_vm_running(
    api_socket: &Path,
    timeout_duration: Duration,
) -> Result<(), AgentError> {
    let deadline = Instant::now() + timeout_duration;
    loop {
        if let Ok(body) = firecracker_get_json(api_socket, "/vm").await {
            let state = body
                .get("state")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if state.eq_ignore_ascii_case("running") {
                return Ok(());
            }
        }

        if Instant::now() >= deadline {
            return Err(AgentError::Runtime(format!(
                "firecracker launch timeout waiting for vm state running: path={} timeout_ms={}",
                api_socket.display(),
                timeout_duration.as_millis()
            )));
        }

        sleep(Duration::from_millis(API_POLL_INTERVAL_MS)).await;
    }
}

async fn probe_guest_readiness(
    read_half: &mut BufReader<OwnedReadHalf>,
    write_half: &mut OwnedWriteHalf,
    timeout_duration: Duration,
) -> Result<(), AgentError> {
    write_half
        .write_all(READY_PROBE_PAYLOAD.as_bytes())
        .await
        .map_err(|err| AgentError::Runtime(format!("write firecracker ready probe failed: {err}")))?;
    write_half
        .write_all(b"\n")
        .await
        .map_err(|err| AgentError::Runtime(format!("write firecracker ready probe newline failed: {err}")))?;
    write_half
        .flush()
        .await
        .map_err(|err| AgentError::Runtime(format!("flush firecracker ready probe failed: {err}")))?;

    let mut response_line = String::new();
    let read = timeout(timeout_duration, read_half.read_line(&mut response_line))
        .await
        .map_err(|_| {
            AgentError::Runtime(format!(
                "firecracker guest readiness probe timed out after {}ms",
                timeout_duration.as_millis()
            ))
        })?
        .map_err(|err| AgentError::Runtime(format!("read firecracker ready probe failed: {err}")))?;

    if read == 0 {
        return Err(AgentError::Runtime(
            "firecracker guest readiness probe connection closed".to_string(),
        ));
    }

    let response = serde_json::from_str::<serde_json::Value>(response_line.trim_end()).map_err(|err| {
        AgentError::Protocol(format!("decode firecracker ready probe response failed: {err}"))
    })?;
    let status = response
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if status != "ok" {
        return Err(AgentError::Runtime(format!(
            "firecracker guest readiness probe returned non-ok status: {status}"
        )));
    }

    Ok(())
}

async fn firecracker_put_json(
    api_socket: &Path,
    path: &str,
    payload: &serde_json::Value,
) -> Result<(), AgentError> {
    let body = serde_json::to_vec(payload)
        .map_err(|err| AgentError::Protocol(format!("encode firecracker request failed: {err}")))?;
    let request = format!(
        "PUT {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    let mut stream = UnixStream::connect(api_socket).await.map_err(|err| {
        AgentError::Runtime(format!(
            "connect firecracker api socket failed: path={} error={err}",
            api_socket.display()
        ))
    })?;
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|err| AgentError::Runtime(format!("write firecracker api request failed: {err}")))?;
    stream
        .write_all(&body)
        .await
        .map_err(|err| AgentError::Runtime(format!("write firecracker api body failed: {err}")))?;
    stream
        .flush()
        .await
        .map_err(|err| AgentError::Runtime(format!("flush firecracker api request failed: {err}")))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|err| AgentError::Runtime(format!("read firecracker api response failed: {err}")))?;

    let response_str = String::from_utf8_lossy(&response);
    let status_line = response_str.lines().next().unwrap_or_default();
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or_default();
    if !(200..300).contains(&status_code) {
        return Err(AgentError::Runtime(format!(
            "firecracker api returned non-success status: path={path} status_line={status_line} response={response_str}"
        )));
    }

    Ok(())
}

async fn firecracker_get_json(api_socket: &Path, path: &str) -> Result<serde_json::Value, AgentError> {
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );

    let mut stream = UnixStream::connect(api_socket).await.map_err(|err| {
        AgentError::Runtime(format!(
            "connect firecracker api socket failed: path={} error={err}",
            api_socket.display()
        ))
    })?;
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|err| AgentError::Runtime(format!("write firecracker api request failed: {err}")))?;
    stream
        .flush()
        .await
        .map_err(|err| AgentError::Runtime(format!("flush firecracker api request failed: {err}")))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|err| AgentError::Runtime(format!("read firecracker api response failed: {err}")))?;

    let response_str = String::from_utf8_lossy(&response);
    let (head, body) = response_str.split_once("\r\n\r\n").ok_or_else(|| {
        AgentError::Runtime(format!("invalid firecracker api response: path={path}"))
    })?;
    let status_line = head.lines().next().unwrap_or_default();
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or_default();
    if !(200..300).contains(&status_code) {
        return Err(AgentError::Runtime(format!(
            "firecracker api returned non-success status: path={path} status_line={status_line} response={response_str}"
        )));
    }

    serde_json::from_str::<serde_json::Value>(body.trim()).map_err(|err| {
        AgentError::Protocol(format!(
            "decode firecracker api json failed: path={path} error={err}"
        ))
    })
}

fn guest_cid_for_agent(agent_id: Uuid) -> u32 {
    let mut bytes = [0_u8; 4];
    bytes.copy_from_slice(&agent_id.as_bytes()[..4]);
    let raw = u32::from_le_bytes(bytes);
    3 + (raw % ((u32::MAX - 3) / 2))
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

fn enforce_network_isolation_policy(config: &FirecrackerVmConfig) -> Result<(), AgentError> {
    if matches!(config.network_policy, NetworkIsolationPolicy::DenyAll) && config.network.is_some()
    {
        let tap_device = config
            .network
            .as_ref()
            .map(|network| network.tap_device.as_str())
            .unwrap_or("<unknown>");
        return Err(AgentError::Permission(format!(
            "jailer network policy denied outbound access: tap={tap_device} nftables=deny_all"
        )));
    }

    Ok(())
}

#[cfg(all(test, feature = "firecracker-integration"))]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn real_firecracker_launch_path_is_available_with_env_assets() {
        let Some(kernel_path) = std::env::var_os("AGENTD_TEST_FIRECRACKER_KERNEL") else {
            return;
        };
        let Some(rootfs_path) = std::env::var_os("AGENTD_TEST_FIRECRACKER_ROOTFS") else {
            return;
        };

        let firecracker_binary = std::env::var_os("AGENTD_TEST_FIRECRACKER_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_FIRECRACKER_BINARY));

        let runtime_root = std::env::temp_dir().join(format!("agentd-fc-it-{}", Uuid::new_v4()));
        let executor = FirecrackerExecutor::builder()
            .firecracker_binary(firecracker_binary)
            .kernel_path(PathBuf::from(kernel_path))
            .rootfs_path(PathBuf::from(rootfs_path))
            .vsock_root_dir(runtime_root.join("vsock"))
            .api_socket_root_dir(runtime_root.join("api"))
            .launch_mode(FirecrackerLaunchMode::RealFirecracker)
            .build()
            .expect("build integration firecracker executor");

        let launch = executor
            .launch_agent(FirecrackerAgentLaunchSpec {
                agent_id: Uuid::new_v4(),
                command: "agent-lite".to_string(),
                args: vec![],
                env: HashMap::new(),
                vcpu_count: Some(1),
                mem_size_mib: Some(256),
                network: None,
                network_policy: Some(NetworkIsolationPolicy::AllowAll),
                jailer: None,
                launch_timeout: Duration::from_secs(5),
            })
            .await;

        if let Ok(vm) = launch {
            let _ = vm.shutdown().await;
        }

        let _ = std::fs::remove_dir_all(runtime_root);
    }
}
