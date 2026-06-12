use reqwest::Client;
use std::sync::RwLock;

pub struct Telegram {
    client: Client,
    token: String,
    chat_id: RwLock<String>,
}

impl Telegram {
    pub fn new(token: String, chat_id: String) -> Self {
        Self {
            client: Client::new(),
            token,
            chat_id: RwLock::new(chat_id),
        }
    }

    pub async fn send(&self, text: &str) -> anyhow::Result<()> {
        let chat_id = self
            .chat_id
            .read()
            .map_err(|_| anyhow::anyhow!("telegram chat_id lock poisoned"))?
            .clone();

        if let Some(new_id) = self.try_send(text, &chat_id).await? {
            tracing::warn!(
                "Telegram chat migrated to supergroup {new_id}; \
                 set TELEGRAM_CHAT_ID={new_id} to persist across restarts"
            );
            *self
                .chat_id
                .write()
                .map_err(|_| anyhow::anyhow!("telegram chat_id lock poisoned"))? = new_id.clone();
            self.try_send(text, &new_id).await?;
        }

        Ok(())
    }

    async fn try_send(&self, text: &str, chat_id: &str) -> anyhow::Result<Option<String>> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);

        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "HTML",
            }))
            .send()
            .await?;

        let status = resp.status();

        let body = resp.text().await.unwrap_or_default();

        if status.is_success() {
            tracing::debug!("Telegram message sent");
            return Ok(None);
        }

        if let Some(new_id) = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["parameters"]["migrate_to_chat_id"].as_i64())
        {
            return Ok(Some(new_id.to_string()));
        }

        tracing::error!("Telegram API error: {status} - {body}");
        Ok(None)
    }
}
