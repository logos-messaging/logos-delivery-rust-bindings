use std::time::Duration;

use serde_json::{json, Map, Value};

/// Node configuration, serialised to the JSON `logosdelivery_create_node` takes.
#[derive(Clone)]
pub struct DeliveryConfig {
    preset: Option<String>,
    mode: String,
    tcp_port: u16,
    discv5_udp_port: Option<u16>,
    log_level: String,
    call_timeout: Duration,
    connect_wait: Option<Duration>,
    skip_version_check: bool,
    min_library_version: Option<String>,
    extra: Map<String, Value>,
}

// Not derived: `extra` can hold secrets (node keys, credential passwords), so only
// its keys are shown.
impl std::fmt::Debug for DeliveryConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeliveryConfig")
            .field("preset", &self.preset)
            .field("mode", &self.mode)
            .field("tcp_port", &self.tcp_port)
            .field("discv5_udp_port", &self.discv5_udp_port)
            .field("log_level", &self.log_level)
            .field("call_timeout", &self.call_timeout)
            .field("connect_wait", &self.connect_wait)
            .field("skip_version_check", &self.skip_version_check)
            .field("min_library_version", &self.min_library_version)
            .field("extra_keys", &self.extra.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            preset: Some("logos.dev".into()),
            mode: "Core".into(),
            tcp_port: 0,
            discv5_udp_port: None,
            log_level: "ERROR".into(),
            call_timeout: Duration::from_secs(60),
            connect_wait: None,
            skip_version_check: false,
            min_library_version: None,
            extra: Map::new(),
        }
    }
}

impl DeliveryConfig {
    pub fn preset(mut self, preset: impl Into<String>) -> Self {
        self.preset = Some(preset.into());
        self
    }

    /// Runs without a preset, e.g. for an isolated local network.
    pub fn no_preset(mut self) -> Self {
        self.preset = None;
        self
    }

    /// `"Core"` (default) or `"Edge"`.
    pub fn mode(mut self, mode: impl Into<String>) -> Self {
        self.mode = mode.into();
        self
    }

    pub fn tcp_port(mut self, port: u16) -> Self {
        self.tcp_port = port;
        self
    }

    pub fn discv5_udp_port(mut self, port: u16) -> Self {
        self.discv5_udp_port = Some(port);
        self
    }

    pub fn log_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = level.into();
        self
    }

    /// Upper bound on each FFI call, including `start`.
    pub fn call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
        self
    }

    /// Makes `start` wait up to `timeout` for a first peer before returning. A
    /// node with no peers cannot deliver anything yet, but a slow or empty
    /// network is not a startup failure: on timeout `start` still succeeds.
    pub fn wait_for_connection(mut self, timeout: Duration) -> Self {
        self.connect_wait = Some(timeout);
        self
    }

    /// Any other node option, by config field name (see `get_available_configs`).
    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    /// Skips the startup check that the linked library is at least the version the
    /// bindings were generated from; for custom builds that report another version.
    pub fn skip_version_check(mut self) -> Self {
        self.skip_version_check = true;
        self
    }

    #[doc(hidden)]
    pub fn require_min_library_version(mut self, version: impl Into<String>) -> Self {
        self.min_library_version = Some(version.into());
        self
    }

    pub(crate) fn check_version(&self) -> crate::error::Result<()> {
        if self.skip_version_check {
            return Ok(());
        }
        crate::version::check_library(self.min_library_version.as_deref())
    }

    pub(crate) fn connect_wait(&self) -> Option<Duration> {
        self.connect_wait
    }

    pub(crate) fn timeout(&self) -> Duration {
        self.call_timeout
    }

    /// The flat object the pinned library still accepts next to its layered
    /// `mode`/`preset`/`messagingOverrides` format; the startup version check bounds
    /// which library revisions this has to work with.
    pub(crate) fn to_json(&self) -> String {
        let mut conf = Map::new();
        conf.insert("logLevel".into(), json!(self.log_level));
        conf.insert("mode".into(), json!(self.mode));
        if let Some(preset) = &self.preset {
            conf.insert("preset".into(), json!(preset));
        }
        conf.insert("tcpPort".into(), json!(self.tcp_port));
        // QUIC listens on UDP at the TCP port number, so an explicit TCP port
        // must not double as the discv5 port; an ephemeral one is safe to share.
        if let Some(port) = self.discv5_udp_port.or((self.tcp_port == 0).then_some(0)) {
            conf.insert("discv5UdpPort".into(), json!(port));
        }
        conf.extend(self.extra.clone());
        Value::Object(conf).to_string()
    }
}
