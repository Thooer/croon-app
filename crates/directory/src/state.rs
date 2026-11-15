//! Docker CLI 文件执行，attempt 的持久封印与命名环境共同兑现 retire。
//! 参考：docs/Directory/README.md（文件执行、隔离与恢复）。

use super::*;

impl Docker {
    pub fn new(config: Config) -> Result<Self, HostError> {
        if config.identity.is_empty()
            || config.image.is_empty()
            || config.collector_image.is_empty()
            || config.platform.is_empty()
            || config
                .cpus
                .parse::<f64>()
                .ok()
                .is_none_or(|v| !v.is_finite() || v <= 0.0)
            || config.memory_bytes == 0
            || config.work_bytes == 0
            || config.temporary_bytes == 0
            || config.pids == 0
            || config.output_bytes == 0
            || config.nodes == 0
            || config.archive_bytes == 0
            || config.cleanup_timeout.is_zero()
            || !config.state.is_absolute()
            || !config.docker.is_absolute()
            || !config.python.is_absolute()
        {
            return Err(host(
                "all Docker resources, paths and identity must be explicit and positive",
            ));
        }
        fs::create_dir_all(&config.state).map_err(host)?;
        if fs::symlink_metadata(&config.state)
            .map_err(host)?
            .file_type()
            .is_symlink()
        {
            return Err(host("state is symlink"));
        }
        fs::set_permissions(&config.state, fs::Permissions::from_mode(0o700)).map_err(host)?;
        let digest = croon_eval::value::ValueId::of_bytes(config.identity.as_bytes());
        let prefix = format!("croon-exec-{}", digest.hex());
        let provider = Self {
            config: Rc::new(config),
            prefix,
            runs: Default::default(),
        };
        for image in [&provider.config.image, &provider.config.collector_image] {
            let bytes = provider.run(&[
                "image",
                "inspect",
                "--format",
                "{{json .Config.Volumes}}",
                image,
            ])?;
            let volumes: Json = serde_json::from_slice(&bytes).map_err(host)?;
            if !volumes.is_null() && volumes.as_object().is_none_or(|v| !v.is_empty()) {
                return Err(host("image declares writable volumes"));
            }
        }
        Ok(provider)
    }
    pub(super) fn name(&self, attempt: &str) -> String {
        format!(
            "{}-{}",
            self.prefix,
            croon_eval::value::ValueId::of_bytes(attempt.as_bytes()).hex()
        )
    }
    pub(super) fn path(&self, attempt: &str) -> PathBuf {
        self.config.state.join(self.name(attempt))
    }
    pub(super) fn lock(&self, attempt: &str) -> Result<Lock, HostError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(
                self.config
                    .state
                    .join(format!("{}.lock", self.name(attempt))),
            )
            .map_err(host)?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(host(std::io::Error::last_os_error()));
        }
        Ok(Lock(file))
    }
    pub(super) fn command(&self) -> Command {
        Command::new(&self.config.docker)
    }
    pub(super) fn run(&self, args: &[&str]) -> Result<Vec<u8>, HostError> {
        let mut cmd = self.command();
        cmd.args(args);
        bounded(cmd, self.config.cleanup_timeout, 1024 * 1024)
    }
    pub(super) fn remove(&self, name: &str) -> Result<(), HostError> {
        // Inspect distinguishes absence from daemon failure; all commands name only this attempt.
        let result = self.run(&[
            "container",
            "ls",
            "-a",
            "--filter",
            &format!("name=^{name}$"),
            "--format",
            "{{.Names}}",
        ])?;
        if String::from_utf8_lossy(&result).lines().any(|n| n == name) {
            self.run(&["rm", "-f", name])?;
        }
        Ok(())
    }
    pub(super) fn remove_id(&self, id: &str) -> Result<(), HostError> {
        let result = self.run(&[
            "container",
            "ls",
            "-a",
            "--filter",
            &format!("id={id}"),
            "--format",
            "{{.ID}}",
        ])?;
        if !result.is_empty() {
            self.run(&["rm", "-f", id])?;
        }
        Ok(())
    }
    pub(super) fn retire_container(&self, name: &str) -> Result<(), HostError> {
        let containers = self.run(&[
            "container",
            "ls",
            "-a",
            "--filter",
            &format!("name=^{name}$"),
            "--format",
            "{{.Names}}",
        ])?;
        if String::from_utf8_lossy(&containers)
            .lines()
            .any(|n| n == name)
        {
            let bytes = self.run(&["inspect", name])?;
            let data: Json = serde_json::from_slice(&bytes).map_err(host)?;
            if data[0]["Config"]["Labels"]["croon.retired"] == name {
                // Never reopen an established name barrier, even if validation now fails.
                return self.tombstone(name);
            }
            self.remove(name)?;
        }
        self.tombstone(name)
    }
    pub(super) fn tombstone(&self, name: &str) -> Result<(), HostError> {
        let containers = self.run(&[
            "container",
            "ls",
            "-a",
            "--filter",
            &format!("name=^{name}$"),
            "--format",
            "{{.Names}}",
        ])?;
        if !String::from_utf8_lossy(&containers)
            .lines()
            .any(|n| n == name)
        {
            self.run(&[
                "create",
                "--pull=never",
                "--name",
                name,
                "--label",
                &format!("croon.retired={name}"),
                "--label",
                &format!("croon.provider={}", self.prefix),
                "--network=none",
                "--read-only",
                "--user=0:0",
                "--cap-drop=ALL",
                "--security-opt=no-new-privileges",
                "--pids-limit",
                &self.config.pids.to_string(),
                "--memory",
                &self.config.memory_bytes.to_string(),
                "--memory-swap",
                &self.config.memory_bytes.to_string(),
                "--cpus",
                &self.config.cpus,
                "--platform",
                &self.config.platform,
                "--log-driver=none",
                "--entrypoint=/bin/true",
                &self.config.image,
            ])?;
        }
        let bytes = self.run(&["inspect", name])?;
        let data: Json = serde_json::from_slice(&bytes).map_err(host)?;
        let c = &data[0];
        if c["Config"]["Labels"]["croon.retired"] != name
            || c["Config"]["Labels"]["croon.provider"] != self.prefix
            || c["Mounts"].as_array().is_none_or(|m| !m.is_empty())
            || c["HostConfig"]["ReadonlyRootfs"] != true
            || c["HostConfig"]["NetworkMode"] != "none"
            || c["HostConfig"]["CapDrop"] != json!(["ALL"])
            || c["Config"]["Entrypoint"] != json!(["/bin/true"])
        {
            return Err(host(
                "retire name was occupied by a non-tombstone; retry retirement",
            ));
        }
        Ok(())
    }
    pub(super) fn seal(&self, path: &Path) -> Result<(), HostError> {
        let marker = path.join("retired");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(marker)
            .map_err(host)?;
        file.sync_all().map_err(host)?;
        File::open(path).map_err(host)?.sync_all().map_err(host)
    }
}
