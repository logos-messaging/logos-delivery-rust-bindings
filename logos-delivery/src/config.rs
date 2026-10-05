use std::time::Duration;

use serde_json::{json, Map, Value};

/// Node configuration, serialised to the JSON `logosdelivery_create_node` takes.
#[derive(Debug, Clone)]
pub struct DeliveryConfig {
    preset: Option<String>,
    mode: String,
    tcp_port: u16,
    discv5_udp_port: Option<u16>,
    log_level: String,
    call_timeout: Duration,
    extra: Map<String, Value>,
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

    /// Any other node option, by config field name (see `get_available_configs`).
    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    pub(crate) fn timeout(&self) -> Duration {
        self.call_timeout
    }

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
