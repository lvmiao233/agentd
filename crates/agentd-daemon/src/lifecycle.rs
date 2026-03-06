use crate::cgroup::{CgroupManager, CgroupResourceLimits};
use crate::firecracker::{
    FirecrackerAgentLaunchSpec, FirecrackerExecutor, FirecrackerNetworkConfig, GuestAgentState,
    JailerConfig, NetworkIsolationPolicy,
};
use agentd_core::AgentError;
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{broadcast, watch, Mutex, RwLock};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

const EVENT_BUFFER_LIMIT: usize = 1000;

#[derive(Debug, Clone)]
pub struct ManagedAgentSpec {
    pub agent_id: Uuid,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub restart_max_attempts: u32,
    pub restart_backoff_secs: u64,
    pub limits: CgroupResourceLimits,
    pub runtime: ManagedRuntimeSpec,
}

#[derive(Debug, Clone)]
pub enum ManagedRuntimeSpec {
    Cgroup,
    Firecracker(FirecrackerRuntimeSpec),
}

#[derive(Debug, Clone)]
pub struct FirecrackerRuntimeSpec {
    pub executor: Arc<FirecrackerExecutor>,
    pub vcpu_count: Option<u8>,
    pub mem_size_mib: Option<u32>,
    pub network: Option<FirecrackerNetworkConfig>,
    pub network_policy: Option<NetworkIsolationPolicy>,
    pub jailer: Option<JailerConfig>,
    pub launch_timeout: Duration,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeKind {
    Cgroup,
    Firecracker,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedAgentState {
    Starting,
    Running,
    Restarting,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedAgentSnapshot {
    pub agent_id: Uuid,
    pub state: ManagedAgentState,
    pub runtime: ManagedRuntimeKind,
    pub pid: Option<u32>,
    pub restart_count: u32,
    pub cgroup_path: String,
    pub limits: CgroupResourceLimits,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LifecycleEvent {
    pub event_id: String,
    pub timestamp: String,
    pub agent_id: Uuid,
    pub event_type: String,
    pub severity: String,
    pub payload: serde_json::Value,
    pub trace_id: String,
}

#[derive(Debug)]
struct ManagedAgentHandle {
    stop_tx: watch::Sender<bool>,
    snapshot: Arc<RwLock<ManagedAgentSnapshot>>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Clone)]
pub struct LifecycleManager {
    cgroup: CgroupManager,
    agents: Arc<Mutex<HashMap<Uuid, ManagedAgentHandle>>>,
    events: Arc<RwLock<Vec<LifecycleEvent>>>,
    event_tx: broadcast::Sender<LifecycleEvent>,
}

impl LifecycleManager {
    pub fn new(cgroup: CgroupManager) -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_BUFFER_LIMIT);
        Self {
            cgroup,
            agents: Arc::new(Mutex::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            event_tx,
        }
    }

    pub async fn start_agent(
        &self,
        spec: ManagedAgentSpec,
    ) -> Result<ManagedAgentSnapshot, AgentError> {
        if spec.command.trim().is_empty() {
            return Err(AgentError::InvalidInput(
                "managed agent command must be non-empty".to_string(),
            ));
        }

        let agent_id = spec.agent_id;
        let runtime = match spec.runtime {
            ManagedRuntimeSpec::Cgroup => ManagedRuntimeKind::Cgroup,
            ManagedRuntimeSpec::Firecracker(_) => ManagedRuntimeKind::Firecracker,
        };

        let cgroup_path_display = if matches!(runtime, ManagedRuntimeKind::Cgroup) {
            let cgroup_path = self.cgroup.ensure_agent_group(agent_id, &spec.limits)?;
            cgroup_path.display().to_string()
        } else {
            format!("firecracker://{agent_id}")
        };

        let mut guard = self.agents.lock().await;
        if guard.contains_key(&agent_id) {
            return Err(AgentError::Runtime(format!(
                "managed agent already exists: {}",
                agent_id
            )));
        }

        let snapshot = Arc::new(RwLock::new(ManagedAgentSnapshot {
            agent_id,
            state: ManagedAgentState::Starting,
            runtime,
            pid: None,
            restart_count: 0,
            cgroup_path: cgroup_path_display,
            limits: spec.limits.clone(),
            command: spec.command.clone(),
            args: spec.args.clone(),
        }));

        let (stop_tx, stop_rx) = watch::channel(false);
        let manager = self.clone();
        let snapshot_clone = snapshot.clone();
        let task = tokio::spawn(async move {
            match spec.runtime.clone() {
                ManagedRuntimeSpec::Cgroup => {
                    manager
                        .run_cgroup_supervisor(spec, snapshot_clone, stop_rx)
                        .await;
                }
                ManagedRuntimeSpec::Firecracker(runtime_spec) => {
                    manager
                        .run_firecracker_supervisor(spec, runtime_spec, snapshot_clone, stop_rx)
                        .await;
                }
            }
        });

        guard.insert(
            agent_id,
            ManagedAgentHandle {
                stop_tx,
                snapshot: snapshot.clone(),
                task,
            },
        );
        drop(guard);

        let initial_snapshot = snapshot.read().await.clone();
        Ok(initial_snapshot)
    }

    pub async fn stop_agent(&self, agent_id: Uuid) -> Result<ManagedAgentSnapshot, AgentError> {
        let handle = {
            let mut guard = self.agents.lock().await;
            guard.remove(&agent_id)
        }
        .ok_or_else(|| AgentError::NotFound(format!("managed agent not found: {agent_id}")))?;

        let _ = handle.stop_tx.send(true);
        let _ = handle.task.await;
        let snapshot = handle.snapshot.read().await.clone();
        Ok(snapshot)
    }

    pub async fn list_agents(&self) -> Vec<ManagedAgentSnapshot> {
        let snapshot_handles = {
            let guard = self.agents.lock().await;
            guard
                .values()
                .map(|handle| handle.snapshot.clone())
                .collect::<Vec<_>>()
        };

        let mut snapshots = Vec::with_capacity(snapshot_handles.len());
        for snapshot in snapshot_handles {
            snapshots.push(snapshot.read().await.clone());
        }
        snapshots
    }

    pub async fn list_events(&self, limit: Option<usize>) -> Vec<LifecycleEvent> {
        let events = self.events.read().await;
        let requested = limit.unwrap_or(events.len());
        let keep = requested.min(events.len());
        events[events.len().saturating_sub(keep)..].to_vec()
    }

    pub async fn list_events_since(
        &self,
        cursor: Option<&str>,
        limit: Option<usize>,
    ) -> Vec<LifecycleEvent> {
        let events = self.events.read().await;
        if events.is_empty() {
            return Vec::new();
        }

        let mut collected = if let Some(cursor_id) = cursor {
            if let Some(position) = events.iter().position(|event| event.event_id == cursor_id) {
                events[position + 1..].to_vec()
            } else {
                let requested = limit.unwrap_or(events.len());
                let keep = requested.min(events.len());
                events[events.len().saturating_sub(keep)..].to_vec()
            }
        } else {
            let requested = limit.unwrap_or(events.len());
            let keep = requested.min(events.len());
            events[events.len().saturating_sub(keep)..].to_vec()
        };

        if let Some(max_items) = limit {
            if collected.len() > max_items {
                collected.truncate(max_items);
            }
        }

        collected
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<LifecycleEvent> {
        self.event_tx.subscribe()
    }

    async fn push_event(&self, event: LifecycleEvent) {
        let mut events = self.events.write().await;
        events.push(event.clone());
        if events.len() > EVENT_BUFFER_LIMIT {
            let drop_n = events.len() - EVENT_BUFFER_LIMIT;
            events.drain(0..drop_n);
        }
        let _ = self.event_tx.send(event);
    }

    async fn run_cgroup_supervisor(
        &self,
        spec: ManagedAgentSpec,
        snapshot: Arc<RwLock<ManagedAgentSnapshot>>,
        mut stop_rx: watch::Receiver<bool>,
    ) {
        let mut restart_count = 0_u32;
        loop {
            if *stop_rx.borrow() {
                break;
            }

            let mut command = Command::new(&spec.command);
            command
                .args(&spec.args)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .kill_on_drop(true);
            for (key, value) in &spec.env {
                command.env(key, value);
            }

            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(err) => {
                    {
                        let mut state = snapshot.write().await;
                        state.state = ManagedAgentState::Failed;
                        state.pid = None;
                        state.restart_count = restart_count;
                    }
                    self.push_event(new_event(
                        spec.agent_id,
                        "agent.start_failed",
                        "error",
                        serde_json::json!({
                            "command": spec.command,
                            "error": err.to_string(),
                        }),
                    ))
                    .await;
                    break;
                }
            };

            let pid = child.id().unwrap_or_default();
            if pid == 0 {
                {
                    let mut state = snapshot.write().await;
                    state.state = ManagedAgentState::Failed;
                    state.pid = None;
                    state.restart_count = restart_count;
                }
                self.push_event(new_event(
                    spec.agent_id,
                    "agent.start_failed",
                    "error",
                    serde_json::json!({
                        "command": spec.command,
                        "error": "spawned process has no pid",
                    }),
                ))
                .await;
                break;
            }

            if let Err(err) = self.cgroup.assign_pid(spec.agent_id, pid) {
                {
                    let mut state = snapshot.write().await;
                    state.state = ManagedAgentState::Failed;
                    state.pid = Some(pid);
                    state.restart_count = restart_count;
                }
                let _ = child.kill().await;
                let _ = child.wait().await;
                self.push_event(new_event(
                    spec.agent_id,
                    "agent.start_failed",
                    "error",
                    serde_json::json!({
                        "pid": pid,
                        "error": err.to_string(),
                    }),
                ))
                .await;
                break;
            }

            let memory_before = self
                .cgroup
                .read_memory_events(spec.agent_id)
                .unwrap_or_default();

            {
                let mut state = snapshot.write().await;
                state.state = if restart_count == 0 {
                    ManagedAgentState::Running
                } else {
                    ManagedAgentState::Restarting
                };
                state.pid = Some(pid);
                state.restart_count = restart_count;
            }

            self.push_event(new_event(
                spec.agent_id,
                "agent.started",
                "info",
                serde_json::json!({
                    "pid": pid,
                    "restart_count": restart_count,
                }),
            ))
            .await;

            let exit_status = tokio::select! {
                changed = stop_rx.changed() => {
                    if changed.is_ok() && *stop_rx.borrow() {
                        let _ = child.kill().await;
                    }
                    child.wait().await
                }
                status = child.wait() => status,
            };

            let memory_after = self
                .cgroup
                .read_memory_events(spec.agent_id)
                .unwrap_or_default();
            if memory_after.oom_detected_since(memory_before) {
                self.push_event(new_event(
                    spec.agent_id,
                    "cgroup.oom",
                    "warning",
                    serde_json::json!({
                        "oom": memory_after.oom,
                        "oom_kill": memory_after.oom_kill,
                    }),
                ))
                .await;
            }

            if *stop_rx.borrow() {
                {
                    let mut state = snapshot.write().await;
                    state.state = ManagedAgentState::Stopped;
                    state.pid = None;
                    state.restart_count = restart_count;
                }
                self.push_event(new_event(
                    spec.agent_id,
                    "agent.stopped",
                    "info",
                    serde_json::json!({"reason": "requested"}),
                ))
                .await;
                break;
            }

            let exited_successfully = match &exit_status {
                Ok(status) => status.success(),
                Err(_) => false,
            };

            match exit_status {
                Ok(status) => {
                    self.push_event(new_event(
                        spec.agent_id,
                        "agent.exited",
                        if status.success() { "info" } else { "warning" },
                        serde_json::json!({
                            "status": status.code(),
                            "success": status.success(),
                        }),
                    ))
                    .await;
                }
                Err(err) => {
                    self.push_event(new_event(
                        spec.agent_id,
                        "agent.exited",
                        "error",
                        serde_json::json!({"error": err.to_string()}),
                    ))
                    .await;
                }
            }

            if exited_successfully {
                {
                    let mut state = snapshot.write().await;
                    state.state = ManagedAgentState::Stopped;
                    state.pid = None;
                    state.restart_count = restart_count;
                }
                self.push_event(new_event(
                    spec.agent_id,
                    "agent.stopped",
                    "info",
                    serde_json::json!({"reason": "exited_successfully"}),
                ))
                .await;
                break;
            }

            if restart_count >= spec.restart_max_attempts {
                {
                    let mut state = snapshot.write().await;
                    state.state = ManagedAgentState::Failed;
                    state.pid = None;
                    state.restart_count = restart_count;
                }
                self.push_event(new_event(
                    spec.agent_id,
                    "agent.restart_exhausted",
                    "error",
                    serde_json::json!({"restart_max_attempts": spec.restart_max_attempts}),
                ))
                .await;
                break;
            }

            restart_count = restart_count.saturating_add(1);
            {
                let mut state = snapshot.write().await;
                state.state = ManagedAgentState::Restarting;
                state.pid = None;
                state.restart_count = restart_count;
            }
            self.push_event(new_event(
                spec.agent_id,
                "agent.restarting",
                "warning",
                serde_json::json!({"attempt": restart_count}),
            ))
            .await;

            sleep(Duration::from_secs(spec.restart_backoff_secs)).await;
        }
    }

    async fn run_firecracker_supervisor(
        &self,
        spec: ManagedAgentSpec,
        runtime_spec: FirecrackerRuntimeSpec,
        snapshot: Arc<RwLock<ManagedAgentSnapshot>>,
        mut stop_rx: watch::Receiver<bool>,
    ) {
        let launch = runtime_spec
            .executor
            .launch_agent(FirecrackerAgentLaunchSpec {
                agent_id: spec.agent_id,
                command: spec.command.clone(),
                args: spec.args.clone(),
                env: spec.env.clone(),
                vcpu_count: runtime_spec.vcpu_count,
                mem_size_mib: runtime_spec.mem_size_mib,
                network: runtime_spec.network.clone(),
                network_policy: runtime_spec.network_policy.clone(),
                jailer: runtime_spec.jailer.clone(),
                launch_timeout: runtime_spec.launch_timeout,
            })
            .await;

        let mut vm = match launch {
            Ok(vm) => vm,
            Err(err) => {
                {
                    let mut state = snapshot.write().await;
                    state.state = ManagedAgentState::Failed;
                    state.pid = None;
                    state.restart_count = 0;
                }
                self.push_event(new_event(
                    spec.agent_id,
                    "agent.start_failed",
                    "error",
                    serde_json::json!({
                        "runtime": "firecracker",
                        "error": err.to_string(),
                    }),
                ))
                .await;
                return;
            }
        };

        {
            let mut state = snapshot.write().await;
            state.state = ManagedAgentState::Running;
            state.pid = vm.pid();
            state.restart_count = 0;
        }

        self.push_event(new_event(
            spec.agent_id,
            "agent.started",
            "info",
            serde_json::json!({
                "runtime": "firecracker",
                "pid": vm.pid(),
            }),
        ))
        .await;

        loop {
            tokio::select! {
                changed = stop_rx.changed() => {
                    if changed.is_err() || *stop_rx.borrow() {
                        break;
                    }
                }
                _ = sleep(Duration::from_millis(150)) => {
                    if let Ok(guest_status) = vm.guest_status(Duration::from_millis(300)).await {
                        if matches!(guest_status.state, GuestAgentState::Exited) {
                            let exit_code = guest_status.exit_code;
                            let event_type = if exit_code == Some(0) {
                                "agent.exited"
                            } else {
                                "agent.failed"
                            };
                            let severity = if exit_code == Some(0) { "info" } else { "error" };
                            let next_state = if exit_code == Some(0) {
                                ManagedAgentState::Stopped
                            } else {
                                ManagedAgentState::Failed
                            };

                            {
                                let mut state = snapshot.write().await;
                                state.state = next_state;
                                state.pid = None;
                            }

                            self.push_event(new_event(
                                spec.agent_id,
                                event_type,
                                severity,
                                serde_json::json!({
                                    "runtime": "firecracker",
                                    "pid": guest_status.pid,
                                    "exit_code": exit_code,
                                }),
                            ))
                            .await;
                            return;
                        }
                    }

                    if let Some(pid) = vm.pid() {
                        if !std::path::PathBuf::from(format!("/proc/{pid}")).exists() {
                                {
                                    let mut state = snapshot.write().await;
                                    state.state = ManagedAgentState::Stopped;
                                    state.pid = None;
                                }
                                self.push_event(new_event(
                                    spec.agent_id,
                                    "agent.exited",
                                    "info",
                                    serde_json::json!({
                                        "runtime": "firecracker",
                                        "pid": pid,
                                    }),
                                ))
                                .await;
                                return;
                        }
                    }
                }
            }
        }

        let shutdown_result = vm.shutdown().await;
        {
            let mut state = snapshot.write().await;
            state.state = ManagedAgentState::Stopped;
            state.pid = None;
        }

        match shutdown_result {
            Ok(()) => {
                self.push_event(new_event(
                    spec.agent_id,
                    "agent.stopped",
                    "info",
                    serde_json::json!({"runtime": "firecracker", "reason": "requested"}),
                ))
                .await;
            }
            Err(err) => {
                self.push_event(new_event(
                    spec.agent_id,
                    "agent.stop_failed",
                    "error",
                    serde_json::json!({"runtime": "firecracker", "error": err.to_string()}),
                ))
                .await;
            }
        }
    }
}

fn new_event(
    agent_id: Uuid,
    event_type: &str,
    severity: &str,
    payload: serde_json::Value,
) -> LifecycleEvent {
    let event_id = Uuid::new_v4();
    LifecycleEvent {
        event_id: event_id.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        agent_id,
        event_type: event_type.to_string(),
        severity: severity.to_string(),
        payload,
        trace_id: format!("trace-{event_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cgroup_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("agentd-lifecycle-test-{}", Uuid::new_v4()))
    }

    fn test_manager(root: &std::path::Path) -> LifecycleManager {
        LifecycleManager::new(CgroupManager::new(root, "agentd"))
    }

    #[tokio::test]
    async fn lifecycle_start_stop_and_events_work() {
        let root = temp_cgroup_root();
        let manager = test_manager(&root);
        let agent_id = Uuid::new_v4();

        let started = manager
            .start_agent(ManagedAgentSpec {
                agent_id,
                command: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "sleep 2".to_string()],
                env: HashMap::new(),
                restart_max_attempts: 0,
                restart_backoff_secs: 0,
                limits: CgroupResourceLimits::default(),
                runtime: ManagedRuntimeSpec::Cgroup,
            })
            .await
            .expect("start managed agent should succeed");
        assert_eq!(started.agent_id, agent_id);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let listed = manager.list_agents().await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].agent_id, agent_id);

        let stopped = manager
            .stop_agent(agent_id)
            .await
            .expect("stop managed agent should succeed");
        assert_eq!(stopped.agent_id, agent_id);
        assert!(matches!(stopped.state, ManagedAgentState::Stopped));

        let events = manager.list_events(None).await;
        assert!(events.iter().any(|e| e.event_type == "agent.started"));
        assert!(events.iter().any(|e| e.event_type == "agent.stopped"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn lifecycle_restart_and_oom_event_are_reported() {
        let root = temp_cgroup_root();
        let manager = test_manager(&root);
        let agent_id = Uuid::new_v4();

        manager
            .start_agent(ManagedAgentSpec {
                agent_id,
                command: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "sleep 0.2; exit 1".to_string()],
                env: HashMap::new(),
                restart_max_attempts: 1,
                restart_backoff_secs: 0,
                limits: CgroupResourceLimits::default(),
                runtime: ManagedRuntimeSpec::Cgroup,
            })
            .await
            .expect("start should succeed");

        tokio::time::sleep(Duration::from_millis(50)).await;
        let memory_events_path = root
            .join("agentd")
            .join(agent_id.to_string())
            .join("memory.events");
        std::fs::write(memory_events_path, "oom 1\noom_kill 1\n")
            .expect("write simulated oom events");

        tokio::time::sleep(Duration::from_millis(700)).await;
        let events = manager.list_events(None).await;
        assert!(events.iter().any(|e| e.event_type == "cgroup.oom"));
        assert!(events.iter().any(|e| e.event_type == "agent.restarting"));
        assert!(events
            .iter()
            .any(|e| e.event_type == "agent.restart_exhausted"));

        let _ = manager.stop_agent(agent_id).await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn list_events_since_cursor_and_subscription_work() {
        let root = temp_cgroup_root();
        let manager = test_manager(&root);
        let agent_id = Uuid::new_v4();

        let mut subscription = manager.subscribe_events();
        manager
            .start_agent(ManagedAgentSpec {
                agent_id,
                command: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "sleep 0.1".to_string()],
                env: HashMap::new(),
                restart_max_attempts: 0,
                restart_backoff_secs: 0,
                limits: CgroupResourceLimits::default(),
                runtime: ManagedRuntimeSpec::Cgroup,
            })
            .await
            .expect("start managed agent should succeed");

        let first_event = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
            .await
            .expect("subscription should receive event in time")
            .expect("subscription should receive valid event");
        assert_eq!(first_event.agent_id, agent_id);

        tokio::time::sleep(Duration::from_millis(300)).await;
        let replayed = manager
            .list_events_since(Some(&first_event.event_id), Some(20))
            .await;
        assert!(
            replayed
                .iter()
                .all(|event| event.event_id != first_event.event_id),
            "replayed events should exclude cursor event"
        );

        let _ = manager.stop_agent(agent_id).await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn successful_exit_does_not_trigger_restart_loop() {
        let root = temp_cgroup_root();
        let manager = test_manager(&root);
        let agent_id = Uuid::new_v4();

        manager
            .start_agent(ManagedAgentSpec {
                agent_id,
                command: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "exit 0".to_string()],
                env: HashMap::new(),
                restart_max_attempts: 3,
                restart_backoff_secs: 0,
                limits: CgroupResourceLimits::default(),
                runtime: ManagedRuntimeSpec::Cgroup,
            })
            .await
            .expect("start should succeed");

        tokio::time::sleep(Duration::from_millis(200)).await;

        let events = manager.list_events(None).await;
        assert!(events.iter().any(|e| e.event_type == "agent.exited"));
        assert!(events.iter().any(
            |e| e.event_type == "agent.stopped" && e.payload["reason"] == "exited_successfully"
        ));
        assert!(!events.iter().any(|e| e.event_type == "agent.restarting"));

        let _ = manager.stop_agent(agent_id).await;
        let _ = std::fs::remove_dir_all(root);
    }
}
