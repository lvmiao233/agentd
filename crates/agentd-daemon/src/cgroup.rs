use agentd_core::AgentError;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct CgroupResourceLimits {
    pub cpu_weight: u64,
    pub memory_high: String,
    pub memory_max: String,
}

impl Default for CgroupResourceLimits {
    fn default() -> Self {
        Self {
            cpu_weight: 100,
            memory_high: "256M".to_string(),
            memory_max: "512M".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryEvents {
    pub oom: u64,
    pub oom_kill: u64,
}

impl MemoryEvents {
    pub fn oom_detected_since(&self, previous: MemoryEvents) -> bool {
        self.oom > previous.oom || self.oom_kill > previous.oom_kill
    }
}

#[derive(Debug, Clone)]
pub struct CgroupManager {
    root: PathBuf,
    parent: String,
}

impl CgroupManager {
    pub fn new(root: impl Into<PathBuf>, parent: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            parent: parent.into(),
        }
    }

    pub fn agent_group_path(&self, agent_id: Uuid) -> PathBuf {
        self.resolved_root()
            .join(&self.parent)
            .join(agent_id.to_string())
    }

    pub fn ensure_agent_group(
        &self,
        agent_id: Uuid,
        limits: &CgroupResourceLimits,
    ) -> Result<PathBuf, AgentError> {
        let parent_path = self.resolved_root().join(&self.parent);
        std::fs::create_dir_all(&parent_path).map_err(|err| {
            AgentError::Runtime(format!(
                "create cgroup parent directory failed: path={} error={err}",
                parent_path.display()
            ))
        })?;

        let group_path = self.agent_group_path(agent_id);
        std::fs::create_dir_all(&group_path).map_err(|err| {
            AgentError::Runtime(format!(
                "create agent cgroup directory failed: path={} error={err}",
                group_path.display()
            ))
        })?;

        write_limit_file(
            &group_path.join("cpu.weight"),
            &limits.cpu_weight.to_string(),
        )?;
        write_limit_file(&group_path.join("memory.high"), &limits.memory_high)?;
        write_limit_file(&group_path.join("memory.max"), &limits.memory_max)?;

        let memory_events_path = group_path.join("memory.events");
        if !memory_events_path.exists() {
            match std::fs::write(&memory_events_path, "oom 0\noom_kill 0\n") {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {}
                Err(err) => {
                    return Err(AgentError::Runtime(format!(
                        "initialize memory.events failed: path={} error={err}",
                        memory_events_path.display()
                    )));
                }
            }
        }

        Ok(group_path)
    }

    pub fn assign_pid(&self, agent_id: Uuid, pid: u32) -> Result<(), AgentError> {
        let procs_path = self.agent_group_path(agent_id).join("cgroup.procs");
        write_limit_file(&procs_path, &pid.to_string())
    }

    pub fn read_memory_events(&self, agent_id: Uuid) -> Result<MemoryEvents, AgentError> {
        let path = self.agent_group_path(agent_id).join("memory.events");
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(parse_memory_events(&content)),
            Err(err)
                if err.kind() == std::io::ErrorKind::NotFound
                    || err.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                Ok(MemoryEvents::default())
            }
            Err(err) => Err(AgentError::Runtime(format!(
                "read memory.events failed: path={} error={err}",
                path.display()
            ))),
        }
    }

    fn resolved_root(&self) -> PathBuf {
        if self.root == Path::new("/sys/fs/cgroup") {
            if let Some(path) = writable_user_service_cgroup_root() {
                return path;
            }
        }

        self.root.clone()
    }
}

fn writable_user_service_cgroup_root() -> Option<PathBuf> {
    let uid = current_uid()?;
    let candidate = user_service_cgroup_root_for_uid(uid);
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

fn user_service_cgroup_root_for_uid(uid: u32) -> PathBuf {
    PathBuf::from(format!(
        "/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service"
    ))
}

fn current_uid() -> Option<u32> {
    if let Ok(value) = std::env::var("UID") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            return Some(parsed);
        }
    }

    let output = Command::new("id").arg("-u").output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
}

fn write_limit_file(path: &Path, value: &str) -> Result<(), AgentError> {
    match std::fs::write(path, format!("{value}\n")) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => Ok(()),
        Err(err) => Err(AgentError::Runtime(format!(
            "write cgroup control file failed: path={} value={} error={err}",
            path.display(),
            value
        ))),
    }
}

fn parse_memory_events(content: &str) -> MemoryEvents {
    let mut events = MemoryEvents::default();
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next();
        let value = parts.next();
        if let (Some(key), Some(value)) = (key, value) {
            if let Ok(parsed) = value.parse::<u64>() {
                match key {
                    "oom" => events.oom = parsed,
                    "oom_kill" => events.oom_kill = parsed,
                    _ => {}
                }
            }
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cgroup_root() -> PathBuf {
        std::env::temp_dir().join(format!("agentd-cgroup-manager-test-{}", Uuid::new_v4()))
    }

    #[test]
    fn ensure_group_writes_limits_and_assigns_pid() {
        let root = temp_cgroup_root();
        let manager = CgroupManager::new(&root, "agentd");
        let agent_id = Uuid::new_v4();

        let limits = CgroupResourceLimits {
            cpu_weight: 200,
            memory_high: "128M".to_string(),
            memory_max: "256M".to_string(),
        };

        let group_path = manager
            .ensure_agent_group(agent_id, &limits)
            .expect("ensure cgroup should succeed");
        manager
            .assign_pid(agent_id, 4242)
            .expect("assign pid should succeed");

        assert_eq!(
            std::fs::read_to_string(group_path.join("cpu.weight")).expect("read cpu.weight"),
            "200\n"
        );
        assert_eq!(
            std::fs::read_to_string(group_path.join("memory.high")).expect("read memory.high"),
            "128M\n"
        );
        assert_eq!(
            std::fs::read_to_string(group_path.join("memory.max")).expect("read memory.max"),
            "256M\n"
        );
        assert_eq!(
            std::fs::read_to_string(group_path.join("cgroup.procs")).expect("read cgroup.procs"),
            "4242\n"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parse_memory_events_detects_oom_deltas() {
        let before = parse_memory_events("oom 0\noom_kill 0\n");
        let after = parse_memory_events("oom 1\noom_kill 0\n");
        assert!(after.oom_detected_since(before));

        let after_kill = parse_memory_events("oom 1\noom_kill 2\n");
        assert!(after_kill.oom_detected_since(after));
    }

    #[test]
    fn user_service_cgroup_root_uses_uid_path() {
        assert_eq!(
            user_service_cgroup_root_for_uid(1000),
            PathBuf::from("/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service")
        );
    }
}
