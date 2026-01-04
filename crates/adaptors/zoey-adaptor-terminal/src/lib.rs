use zoey_core::utils::logger::subscribe_logs;
use zoey_core::{AgentRuntime, Result};
use regex::Regex;
use std::sync::{Arc, RwLock};
use tokio::task::JoinHandle;

#[derive(Clone, Default)]
pub struct TerminalConfig {
    pub enabled: bool,
    pub target_filter: Option<String>,
}

pub struct TerminalAdaptor {
    pub config: TerminalConfig,
    pub runtime: Arc<RwLock<AgentRuntime>>,
}

impl TerminalAdaptor {
    pub fn new(config: TerminalConfig, runtime: Arc<RwLock<AgentRuntime>>) -> Self {
        Self { config, runtime }
    }

    pub async fn start(&self) -> Result<Option<JoinHandle<()>>> {
        if !self.config.enabled {
            return Ok(None);
        }
        let mut rx_opt = subscribe_logs();
        let filter = self.config.target_filter.clone().map(|s| s.to_lowercase());
        let handle = tokio::spawn(async move {
            if let Some(rx) = rx_opt.take() {
                let mut rx = rx;
                let patterns = [
                    (Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap(), "sk-REDACTED"),
                    (
                        Regex::new(r"(?i)api[_-]?key\s*[:=]?\s*[A-Za-z0-9-_]{12,}").unwrap(),
                        "api_key=REDACTED",
                    ),
                    (
                        Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap(),
                        "email@redacted",
                    ),
                    (
                        Regex::new(r"\b\+?\d[\d\s-]{8,}\b").unwrap(),
                        "PHONE_REDACTED",
                    ),
                ];
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            let msg_l = ev.message.to_lowercase();
                            let tgt_l = ev.target.to_lowercase();
                            if let Some(f) = &filter {
                                if !msg_l.contains(f) && !tgt_l.contains(f) {
                                    continue;
                                }
                            }
                            let mut msg = ev.message;
                            for (re, rep) in patterns.iter() {
                                msg = re.replace_all(&msg, *rep).into_owned();
                            }
                            println!("[{}][{}] [{}] {}", ev.time, ev.level, ev.target, msg);
                        }
                        Err(_) => break,
                    }
                }
            }
        });
        Ok(Some(handle))
    }
}
