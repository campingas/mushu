use std::{collections::HashMap, process::Output};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    process::Command,
    sync::watch,
    time::{timeout, Duration},
};
use tracing::{info, warn};

use crate::{push::PushStore, wait_for_shutdown};

const HERDR_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const ATTENTION_LINES: &str = "40";
const ATTENTION_MAX_BYTES: usize = 12 * 1024;

async fn herdr_output(command: &mut Command) -> Result<Output> {
    command.kill_on_drop(true);
    timeout(HERDR_COMMAND_TIMEOUT, command.output())
        .await
        .context("herdr command timed out")?
        .context("failed to run herdr")
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Agent {
    pub agent: String,
    #[serde(rename(deserialize = "agent_status"))]
    pub status: String,
    pub cwd: String,
    pub pane_id: String,
    #[serde(rename(deserialize = "terminal_title_stripped"))]
    pub title: String,
    pub state_change_seq: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
    pub number: u32,
    pub focused: bool,
    #[serde(rename(deserialize = "agent_status"))]
    pub status: String,
    pub pane_count: u32,
    pub tab_count: u32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Tab {
    pub tab_id: String,
    pub workspace_id: String,
    pub label: String,
    pub number: u32,
    pub focused: bool,
    #[serde(rename(deserialize = "agent_status"))]
    pub status: String,
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
pub struct Snapshot {
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    #[serde(default)]
    pub tabs: Vec<Tab>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct AttentionChoice {
    pub key: String,
    pub label: String,
}

#[derive(Serialize, Debug)]
pub struct Attention {
    pub agent: String,
    pub title: String,
    pub pane_id: String,
    pub seq: u64,
    pub context: String,
    pub choices: Vec<AttentionChoice>,
}

pub enum AttentionError {
    Gone,
    NotBlocked,
    Changed,
    Failed(anyhow::Error),
}

pub async fn snapshot() -> Result<Snapshot> {
    let mut command = Command::new("herdr");
    command.args(["api", "snapshot"]).env_remove("HERDR_ENV");
    let out = herdr_output(&mut command).await?;
    anyhow::ensure!(out.status.success(), "herdr api snapshot failed");
    let env: SnapshotEnvelope = serde_json::from_slice(&out.stdout)?;
    Ok(env.result.snapshot)
}

pub async fn attention(pane_id: &str) -> Result<Attention, AttentionError> {
    let before = snapshot().await.map_err(AttentionError::Failed)?;
    let agent = before
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)
        .ok_or(AttentionError::Gone)?;
    if agent.status != "blocked" {
        return Err(AttentionError::NotBlocked);
    }
    let before_seq = agent.state_change_seq;

    let mut command = Command::new("herdr");
    command
        .args([
            "agent",
            "read",
            pane_id,
            "--source",
            "detection",
            "--lines",
            ATTENTION_LINES,
            "--format",
            "text",
        ])
        .env_remove("HERDR_ENV");
    let out = herdr_output(&mut command)
        .await
        .map_err(AttentionError::Failed)?;
    if !out.status.success() {
        return Err(AttentionError::Failed(anyhow::anyhow!(
            "herdr agent read failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let context = bounded_context(&String::from_utf8_lossy(&out.stdout));

    // Confirm the same blocked request still owns the pane after reading.
    // Returning a newer sequence with older context could act on a later prompt.
    let after = snapshot().await.map_err(AttentionError::Failed)?;
    let agent = after
        .agents
        .into_iter()
        .find(|agent| agent.pane_id == pane_id)
        .ok_or(AttentionError::Gone)?;
    if !attention_is_current(&agent, before_seq) {
        if agent.status != "blocked" {
            return Err(AttentionError::NotBlocked);
        }
        return Err(AttentionError::Changed);
    }

    Ok(Attention {
        agent: agent.agent,
        title: agent.title,
        pane_id: agent.pane_id,
        seq: agent.state_change_seq,
        choices: detect_choices(&context),
        context,
    })
}

fn attention_is_current(agent: &Agent, expected_seq: u64) -> bool {
    agent.status == "blocked" && agent.state_change_seq == expected_seq
}

fn completion_changed(previous: &Agent, current: &Agent) -> bool {
    previous.state_change_seq != current.state_change_seq
        && previous.status == "working"
        && matches!(current.status.as_str(), "done" | "idle")
}

fn bounded_label(label: &str) -> String {
    label.chars().take(160).collect()
}

fn bounded_context(context: &str) -> String {
    if context.len() <= ATTENTION_MAX_BYTES {
        return context.to_string();
    }
    let mut start = context.len() - ATTENTION_MAX_BYTES;
    while !context.is_char_boundary(start) {
        start += 1;
    }
    context[start..].to_string()
}

fn numbered_choice(line: &str) -> Option<(u8, &str)> {
    let mut line = line.trim();
    for marker in ["›", "❯", "→", ">"] {
        if let Some(rest) = line.strip_prefix(marker) {
            line = rest.trim_start();
            break;
        }
    }
    let digit_end = line.find(|c: char| !c.is_ascii_digit())?;
    let number = line[..digit_end].parse::<u8>().ok()?;
    let rest = line.get(digit_end..)?;
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let label = rest.trim();
    (!label.is_empty()).then_some((number, label))
}

fn detect_choices(context: &str) -> Vec<AttentionChoice> {
    let lines: Vec<&str> = context
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    for start in (0..lines.len()).rev() {
        if !matches!(numbered_choice(lines[start]), Some((1, _))) {
            continue;
        }
        if start > 0 && numbered_choice(lines[start - 1]).is_some() {
            continue;
        }
        let mut choices = Vec::new();
        let mut index = start;
        let mut expected = 1u8;
        while index < lines.len() {
            match numbered_choice(lines[index]) {
                Some((number, label)) if number == expected && number <= 9 => {
                    choices.push(AttentionChoice {
                        key: number.to_string(),
                        label: bounded_label(label),
                    });
                    expected += 1;
                    index += 1;
                }
                _ => break,
            }
        }
        // The choices must be a contiguous block near the bottom. Reject a
        // longer numbered menu instead of silently exposing only its first 9.
        if (2..=9).contains(&choices.len())
            && index + 3 >= lines.len()
            && !lines[index..]
                .iter()
                .any(|line| numbered_choice(line).is_some())
        {
            return choices;
        }
    }
    Vec::new()
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
    if let Some(scope) = match req.action.as_str() {
        "focus-workspace" => Some("workspace"),
        "focus-tab" => Some("tab"),
        "focus-agent" => Some("agent"),
        _ => None,
    } {
        if !req
            .text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
        {
            return Err(ActionError::Invalid("bad target id"));
        }
        let mut command = Command::new("herdr");
        command
            .args([scope, "focus", &req.text])
            .env_remove("HERDR_ENV");
        let out = herdr_output(&mut command)
            .await
            .map_err(ActionError::Failed)?;
        audit(
            req,
            scope,
            if out.status.success() {
                "ok"
            } else {
                "herdr-error"
            },
        )
        .await;
        return if out.status.success() {
            Ok(())
        } else {
            Err(ActionError::Failed(anyhow::anyhow!(
                String::from_utf8_lossy(&out.stderr).to_string()
            )))
        };
    }
    let agents = snapshot().await.map_err(ActionError::Failed)?.agents;
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
            if keys.is_empty()
                || !keys
                    .iter()
                    .all(|k| k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            {
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
    let out = herdr_output(&mut cmd).await.map_err(ActionError::Failed)?;
    let result = if out.status.success() {
        "ok"
    } else {
        "herdr-error"
    };
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
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all(entry.as_bytes());
        }
    })
    .await;
}

#[derive(Default)]
struct AttentionState {
    blocked_snapshots: u8,
    nonblocked_snapshots: u8,
    latched: bool,
}

#[derive(Default)]
struct AttentionTracker {
    panes: HashMap<String, AttentionState>,
}

impl AttentionTracker {
    fn seed(&mut self, agents: &[Agent]) {
        self.panes = agents
            .iter()
            .map(|agent| {
                (
                    agent.pane_id.clone(),
                    AttentionState {
                        blocked_snapshots: u8::from(agent.status == "blocked") * 2,
                        nonblocked_snapshots: 0,
                        latched: agent.status == "blocked",
                    },
                )
            })
            .collect();
    }

    fn observe(&mut self, agents: &[Agent]) -> Vec<Agent> {
        let mut notifications = Vec::new();
        let present: std::collections::HashSet<&str> =
            agents.iter().map(|agent| agent.pane_id.as_str()).collect();
        self.panes
            .retain(|pane_id, _| present.contains(pane_id.as_str()));

        for agent in agents {
            let state = self.panes.entry(agent.pane_id.clone()).or_default();
            if agent.status == "blocked" {
                state.nonblocked_snapshots = 0;
                state.blocked_snapshots = state.blocked_snapshots.saturating_add(1).min(2);
                if state.blocked_snapshots == 2 && !state.latched {
                    state.latched = true;
                    notifications.push(agent.clone());
                }
            } else {
                state.blocked_snapshots = 0;
                if state.latched {
                    state.nonblocked_snapshots =
                        state.nonblocked_snapshots.saturating_add(1).min(2);
                    if state.nonblocked_snapshots == 2 {
                        state.latched = false;
                        state.nonblocked_snapshots = 0;
                    }
                }
            }
        }
        notifications
    }
}

/// Poll Herdr and send one notification for each stable attention incident.
pub async fn notifier_loop(host: String, push: PushStore, mut shutdown: watch::Receiver<bool>) {
    let mut version_command = Command::new("herdr");
    version_command.arg("--version").kill_on_drop(true);
    let version = tokio::select! {
        result = version_command.output() => result,
        _ = wait_for_shutdown(&mut shutdown) => return,
    };
    if version.is_err() {
        info!("herdr not found, agent notifier disabled");
        return;
    }
    let mut tracker = AttentionTracker::default();
    let mut last: HashMap<String, Agent> = HashMap::new();
    let mut first_run = true;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}
            _ = wait_for_shutdown(&mut shutdown) => break,
        }
        let snapshot = tokio::select! {
            result = snapshot() => result,
            _ = wait_for_shutdown(&mut shutdown) => break,
        };
        let agents = match snapshot {
            Ok(s) => s.agents,
            Err(e) => {
                warn!("snapshot failed: {e:#}");
                continue;
            }
        };
        if first_run {
            tracker.seed(&agents);
            last = agents
                .into_iter()
                .map(|agent| (agent.pane_id.clone(), agent))
                .collect();
            first_run = false;
            continue;
        }
        for a in tracker.observe(&agents) {
            let dir = short_dir(&a.cwd);
            let title = format!("{} needs you", a.agent);
            let body = format!("{} · {} is waiting for input", dir, a.title);
            push.send_attention(&title, &body, &host, &a.pane_id, a.state_change_seq)
                .await;
        }
        for agent in &agents {
            let Some(previous) = last.get(&agent.pane_id) else {
                continue;
            };
            if completion_changed(previous, agent) {
                let dir = short_dir(&agent.cwd);
                push.send_to_all(
                    &format!("{} finished", agent.agent),
                    &format!("{} · {}", dir, agent.title),
                    &host,
                )
                .await;
            }
        }
        last = agents
            .into_iter()
            .map(|agent| (agent.pane_id.clone(), agent))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        attention_is_current, bounded_context, completion_changed, detect_choices, Agent,
        AttentionChoice, AttentionTracker,
    };

    fn agent(pane_id: &str, status: &str, seq: u64) -> Agent {
        Agent {
            agent: "codex".into(),
            status: status.into(),
            cwd: "/tmp/project".into(),
            pane_id: pane_id.into(),
            title: "task".into(),
            state_change_seq: seq,
        }
    }

    #[test]
    fn attention_latches_after_two_blocked_snapshots_despite_seq_changes() {
        let mut tracker = AttentionTracker::default();
        assert!(tracker.observe(&[agent("p1", "blocked", 1)]).is_empty());
        assert_eq!(tracker.observe(&[agent("p1", "blocked", 2)]).len(), 1);
        assert!(tracker.observe(&[agent("p1", "blocked", 3)]).is_empty());
        assert!(tracker.observe(&[agent("p1", "working", 4)]).is_empty());
        assert!(tracker.observe(&[agent("p1", "blocked", 5)]).is_empty());
    }

    #[test]
    fn attention_clears_after_two_nonblocked_snapshots() {
        let mut tracker = AttentionTracker::default();
        tracker.observe(&[agent("p1", "blocked", 1)]);
        tracker.observe(&[agent("p1", "blocked", 2)]);
        tracker.observe(&[agent("p1", "working", 3)]);
        tracker.observe(&[agent("p1", "idle", 4)]);
        assert!(tracker.observe(&[agent("p1", "blocked", 5)]).is_empty());
        assert_eq!(tracker.observe(&[agent("p1", "blocked", 6)]).len(), 1);
    }

    #[test]
    fn pane_removal_clears_latch_and_panes_are_independent() {
        let mut tracker = AttentionTracker::default();
        tracker.observe(&[agent("p1", "blocked", 1), agent("p2", "blocked", 1)]);
        assert_eq!(tracker.observe(&[agent("p1", "blocked", 2)]).len(), 1);
        assert!(tracker.observe(&[]).is_empty());
        assert!(tracker.observe(&[agent("p1", "blocked", 3)]).is_empty());
        assert_eq!(tracker.observe(&[agent("p1", "blocked", 4)]).len(), 1);
    }

    #[test]
    fn seeding_suppresses_already_blocked_panes_after_restart() {
        let mut tracker = AttentionTracker::default();
        tracker.seed(&[agent("p1", "blocked", 1)]);
        assert!(tracker.observe(&[agent("p1", "blocked", 1)]).is_empty());
        assert!(tracker.observe(&[agent("p1", "blocked", 2)]).is_empty());
    }

    #[test]
    fn choices_require_consecutive_numbering_near_bottom() {
        assert_eq!(
            detect_choices("Question\n1. Alpha\n2) Beta\nfooter"),
            vec![
                AttentionChoice {
                    key: "1".into(),
                    label: "Alpha".into()
                },
                AttentionChoice {
                    key: "2".into(),
                    label: "Beta".into()
                },
            ]
        );
        assert!(detect_choices("1. Alpha\n3. Gamma").is_empty());
        assert_eq!(
            detect_choices("Question\n› 1. Alpha\n2. Beta"),
            vec![
                AttentionChoice {
                    key: "1".into(),
                    label: "Alpha".into()
                },
                AttentionChoice {
                    key: "2".into(),
                    label: "Beta".into()
                },
            ]
        );
        assert!(detect_choices("1. old A\n2. old B\nNew question\n1. current item").is_empty());
        assert!(detect_choices("1. Alpha\n2. Beta\na\nb\nc\nd").is_empty());
        assert!(detect_choices("1. Only").is_empty());
        assert!(detect_choices("0. Zero\n1. One\n2. Two").is_empty());
        assert!(
            detect_choices("1. A\n2. B\n3. C\n4. D\n5. E\n6. F\n7. G\n8. H\n9. I\n10. J")
                .is_empty()
        );
    }

    #[test]
    fn context_keeps_at_most_twelve_kibibytes_on_utf8_boundary() {
        let input = format!("old{}new", "é".repeat(7000));
        let bounded = bounded_context(&input);
        assert!(bounded.len() <= 12 * 1024);
        assert!(bounded.ends_with("new"));
    }

    #[test]
    fn changed_attention_sequence_is_not_current() {
        assert!(attention_is_current(&agent("p1", "blocked", 7), 7));
        assert!(!attention_is_current(&agent("p1", "blocked", 8), 7));
        assert!(!attention_is_current(&agent("p1", "working", 7), 7));
    }

    #[test]
    fn completion_notifications_only_follow_working_state_changes() {
        assert!(completion_changed(
            &agent("p1", "working", 1),
            &agent("p1", "done", 2)
        ));
        assert!(completion_changed(
            &agent("p1", "working", 1),
            &agent("p1", "idle", 2)
        ));
        assert!(!completion_changed(
            &agent("p1", "blocked", 1),
            &agent("p1", "idle", 2)
        ));
    }
}
