use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{info, warn};

use crate::push::PushStore;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Agent {
    pub agent: String,
    #[serde(rename = "agent_status")]
    pub status: String,
    pub cwd: String,
    pub pane_id: String,
    #[serde(rename = "terminal_title_stripped")]
    pub title: String,
    pub state_change_seq: u64,
}

#[derive(Deserialize)]
struct SnapshotEnvelope {
    result: SnapshotResult,
}
#[derive(Deserialize)]
struct SnapshotResult {
    snapshot: Snapshot,
}
#[derive(Deserialize)]
struct Snapshot {
    agents: Vec<Agent>,
}

pub async fn snapshot() -> Result<Vec<Agent>> {
    let out = Command::new("herdr")
        .args(["api", "snapshot"])
        .env_remove("HERDR_ENV")
        .output()
        .await
        .context("failed to run herdr")?;
    anyhow::ensure!(out.status.success(), "herdr api snapshot failed");
    let env: SnapshotEnvelope = serde_json::from_slice(&out.stdout)?;
    Ok(env.result.snapshot.agents)
}

fn short_dir(cwd: &str) -> String {
    cwd.rsplit('/').next().unwrap_or(cwd).to_string()
}

#[derive(Deserialize)]
pub struct ActionRequest {
    pub pane_id: String,
    pub seq: u64,
    pub action: String, // "keys" or "prompt"
    pub text: String,
}

pub enum ActionError {
    Gone,
    Stale,
    Invalid(&'static str),
    Failed(anyhow::Error),
}

pub async fn run_action(req: &ActionRequest) -> Result<(), ActionError> {
    if req.text.len() > 4096 {
        return Err(ActionError::Invalid("text too long"));
    }
    let agents = snapshot().await.map_err(ActionError::Failed)?;
    let agent = agents
        .iter()
        .find(|a| a.pane_id == req.pane_id)
        .ok_or(ActionError::Gone)?;
    // Reject actions aimed at a state the phone was no longer looking at.
    if agent.state_change_seq != req.seq {
        return Err(ActionError::Stale);
    }

    let mut cmd = Command::new("herdr");
    cmd.env_remove("HERDR_ENV");
    match req.action.as_str() {
        "keys" => {
            let keys: Vec<&str> = req.text.split_whitespace().collect();
            if keys.is_empty() || !keys.iter().all(|k| k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
                return Err(ActionError::Invalid("bad key names"));
            }
            cmd.args(["agent", "send-keys", &req.pane_id]).args(&keys);
        }
        "prompt" => {
            if req.text.trim().is_empty() {
                return Err(ActionError::Invalid("empty prompt"));
            }
            cmd.args(["agent", "prompt", &req.pane_id, &req.text]);
        }
        _ => return Err(ActionError::Invalid("unknown action")),
    }
    let out = cmd.output().await.map_err(|e| ActionError::Failed(e.into()))?;
    let result = if out.status.success() { "ok" } else { "herdr-error" };
    audit(req, &agent.agent, result).await;
    if out.status.success() {
        Ok(())
    } else {
        Err(ActionError::Failed(anyhow::anyhow!(
            String::from_utf8_lossy(&out.stderr).to_string()
        )))
    }
}

async fn audit(req: &ActionRequest, agent: &str, result: &str) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let line = serde_json::json!({
        "ts": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "pane_id": req.pane_id,
        "agent": agent,
        "action": req.action,
        "text": req.text,
        "result": result,
    });
    let path = std::path::PathBuf::from(home).join(".config/mushu/actions.log");
    let entry = format!("{line}\n");
    let _ = tokio::task::spawn_blocking(move || {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(entry.as_bytes());
        }
    })
    .await;
}

/// Poll herdr and push a notification on notable agent state transitions.
pub async fn notifier_loop(host: String, push: PushStore) {
    if Command::new("herdr").arg("--version").output().await.is_err() {
        info!("herdr not found, agent notifier disabled");
        return;
    }
    let mut last: HashMap<String, Agent> = HashMap::new();
    let mut first_run = true;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let agents = match snapshot().await {
            Ok(a) => a,
            Err(e) => {
                warn!("snapshot failed: {e:#}");
                continue;
            }
        };
        for a in &agents {
            let prev = last.get(&a.pane_id);
            let changed = prev.map_or(true, |p| p.state_change_seq != a.state_change_seq);
            let prev_status = prev.map(|p| p.status.as_str()).unwrap_or("");
            // Skip the initial snapshot so a restart does not replay stale states.
            if first_run || !changed || prev_status == a.status {
                continue;
            }
            let dir = short_dir(&a.cwd);
            let (title, body) = match a.status.as_str() {
                "blocked" => (
                    format!("{} needs you", a.agent),
                    format!("{} · {} is waiting for input", dir, a.title),
                ),
                "done" | "idle" if prev_status == "working" => (
                    format!("{} finished", a.agent),
                    format!("{} · {}", dir, a.title),
                ),
                _ => continue,
            };
            push.send_to_all(&title, &body, &host).await;
        }
        last = agents.into_iter().map(|a| (a.pane_id.clone(), a)).collect();
        first_run = false;
    }
}
