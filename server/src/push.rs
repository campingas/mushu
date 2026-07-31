use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};
use web_push::{
    ContentEncoding, HyperWebPushClient, SubscriptionInfo, VapidSignatureBuilder, WebPushClient,
    WebPushError, WebPushMessageBuilder,
};

#[derive(Serialize, Deserialize, Clone)]
pub struct StoredSubscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

#[derive(Serialize)]
struct NotificationPayload<'a> {
    title: &'a str,
    body: &'a str,
    host: &'a str,
}

#[derive(Clone)]
pub struct PushStore {
    inner: Arc<Mutex<Vec<StoredSubscription>>>,
    subs_path: PathBuf,
    private_key_b64: String,
    pub public_key_b64: String,
    client: HyperWebPushClient,
}

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/mushu")
}

impl PushStore {
    pub fn load_or_init() -> Result<Self> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        let key_path = dir.join("vapid.key");
        let private_key_b64 = if key_path.exists() {
            std::fs::read_to_string(&key_path)?.trim().to_string()
        } else {
            let secret = p256::SecretKey::random(&mut rand_core::OsRng);
            let b64 = URL_SAFE_NO_PAD.encode(secret.to_bytes());
            std::fs::write(&key_path, &b64)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
            }
            info!("generated new VAPID keypair");
            b64
        };
        let secret_bytes = URL_SAFE_NO_PAD
            .decode(&private_key_b64)
            .context("bad vapid.key")?;
        let secret = p256::SecretKey::from_slice(&secret_bytes).context("bad vapid.key")?;
        let public_key_b64 = URL_SAFE_NO_PAD.encode(secret.public_key().to_sec1_bytes());

        let subs_path = dir.join("subscriptions.json");
        let subs: Vec<StoredSubscription> = std::fs::read_to_string(&subs_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        info!("loaded {} push subscription(s)", subs.len());

        Ok(Self {
            inner: Arc::new(Mutex::new(subs)),
            subs_path,
            private_key_b64,
            public_key_b64,
            client: HyperWebPushClient::new(),
        })
    }

    async fn persist(&self, subs: &[StoredSubscription]) {
        if let Ok(json) = serde_json::to_string_pretty(subs) {
            if let Err(e) = tokio::fs::write(&self.subs_path, json).await {
                warn!("failed to persist subscriptions: {e}");
            }
        }
    }

    pub async fn subscribe(&self, sub: StoredSubscription) {
        let mut subs = self.inner.lock().await;
        subs.retain(|s| s.endpoint != sub.endpoint);
        subs.push(sub);
        self.persist(&subs).await;
        info!("push subscription added ({} total)", subs.len());
    }

    pub async fn unsubscribe(&self, endpoint: &str) -> bool {
        let mut subs = self.inner.lock().await;
        let before = subs.len();
        subs.retain(|s| s.endpoint != endpoint);
        let removed = subs.len() != before;
        if removed {
            self.persist(&subs).await;
            info!("push subscription removed ({} total)", subs.len());
        }
        removed
    }

    pub async fn is_subscribed(&self, endpoint: &str) -> bool {
        self.inner
            .lock()
            .await
            .iter()
            .any(|s| s.endpoint == endpoint)
    }

    pub async fn send_to_all(&self, title: &str, body: &str, host: &str) {
        let subs = self.inner.lock().await.clone();
        if subs.is_empty() {
            return;
        }
        let payload = serde_json::to_vec(&NotificationPayload { title, body, host }).unwrap();
        let mut gone: Vec<String> = Vec::new();
        for sub in &subs {
            let info = SubscriptionInfo::new(&sub.endpoint, &sub.p256dh, &sub.auth);
            let result = async {
                let sig = VapidSignatureBuilder::from_base64(
                    &self.private_key_b64,
                    web_push::URL_SAFE_NO_PAD,
                    &info,
                )?
                .build()?;
                let mut msg = WebPushMessageBuilder::new(&info);
                msg.set_vapid_signature(sig);
                msg.set_payload(ContentEncoding::Aes128Gcm, &payload);
                self.client.send(msg.build()?).await
            }
            .await;
            match result {
                Ok(()) => {}
                Err(WebPushError::EndpointNotValid | WebPushError::EndpointNotFound) => {
                    gone.push(sub.endpoint.clone());
                }
                Err(e) => warn!("push send failed: {e}"),
            }
        }
        if !gone.is_empty() {
            let mut subs = self.inner.lock().await;
            subs.retain(|s| !gone.contains(&s.endpoint));
            self.persist(&subs).await;
            info!("pruned {} dead subscription(s)", gone.len());
        }
    }
}
