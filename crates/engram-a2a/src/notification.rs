/// Push notifications — webhook callbacks for async task completion.
///
/// When a task takes too long for synchronous response, the caller
/// provides a push_notification_url. When the task completes, engram
/// sends a POST to that URL with the result.

use crate::task::{TaskResponse, TaskState};

/// A push notification to be sent to a callback URL.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushNotification {
    /// The callback URL to POST to
    pub url: String,
    /// The task response to send
    pub payload: TaskResponse,
}

/// Notification queue for pending deliveries.
#[derive(Debug)]
pub struct NotificationQueue {
    pending: Vec<PushNotification>,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry entries: (notification, attempts_remaining)
    retry: Vec<(PushNotification, u32)>,
}

impl NotificationQueue {
    pub fn new(max_retries: u32) -> Self {
        NotificationQueue {
            pending: Vec::new(),
            max_retries,
            retry: Vec::new(),
        }
    }

    /// Enqueue a notification for delivery.
    pub fn enqueue(&mut self, url: String, response: TaskResponse) {
        self.pending.push(PushNotification {
            url,
            payload: response,
        });
    }

    /// Take all pending notifications for delivery.
    pub fn drain_pending(&mut self) -> Vec<PushNotification> {
        std::mem::take(&mut self.pending)
    }

    /// Report a delivery failure — move to retry queue.
    pub fn report_failure(&mut self, notification: PushNotification) {
        if let Some(entry) = self.retry.iter_mut().find(|(n, _)| n.url == notification.url && n.payload.id == notification.payload.id) {
            if entry.1 > 0 {
                entry.1 -= 1;
            }
        } else {
            self.retry.push((notification, self.max_retries));
        }
    }

    /// Take all notifications eligible for retry.
    pub fn drain_retries(&mut self) -> Vec<PushNotification> {
        let retries: Vec<PushNotification> = self.retry
            .iter()
            .filter(|(_, remaining)| *remaining > 0)
            .map(|(n, _)| n.clone())
            .collect();
        self.retry.retain(|(_, remaining)| *remaining > 0);
        retries
    }

    /// Number of pending notifications.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Number of notifications in retry queue.
    pub fn retry_count(&self) -> usize {
        self.retry.len()
    }

    /// Build the JSON payload for a push notification.
    pub fn build_payload(notification: &PushNotification) -> String {
        serde_json::to_string(&notification.payload).unwrap_or_default()
    }
}

/// Check if a task response should trigger a push notification.
pub fn should_notify(response: &TaskResponse) -> bool {
    matches!(
        response.status.state,
        TaskState::Completed | TaskState::Failed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Artifact, TaskResponse};

    #[test]
    fn enqueue_and_drain() {
        let mut queue = NotificationQueue::new(3);
        queue.enqueue(
            "http://callback.example.com/hook".to_string(),
            TaskResponse::completed("t1", vec![Artifact::json(serde_json::json!({"ok": true}))]),
        );
        assert_eq!(queue.pending_count(), 1);
        let notifications = queue.drain_pending();
        assert_eq!(notifications.len(), 1);
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn retry_on_failure() {
        let mut queue = NotificationQueue::new(3);
        let notification = PushNotification {
            url: "http://callback.example.com/hook".to_string(),
            payload: TaskResponse::completed("t1", vec![]),
        };
        queue.report_failure(notification);
        assert_eq!(queue.retry_count(), 1);
        let retries = queue.drain_retries();
        assert_eq!(retries.len(), 1);
    }

    #[test]
    fn should_notify_completed() {
        let resp = TaskResponse::completed("t1", vec![]);
        assert!(should_notify(&resp));
    }

    #[test]
    fn should_notify_failed() {
        let resp = TaskResponse::failed("t1", "error");
        assert!(should_notify(&resp));
    }

    #[test]
    fn should_not_notify_working() {
        let resp = TaskResponse::working("t1", "in progress");
        assert!(!should_notify(&resp));
    }

    #[test]
    fn build_payload() {
        let notification = PushNotification {
            url: "http://example.com".to_string(),
            payload: TaskResponse::completed("t1", vec![]),
        };
        let json = NotificationQueue::build_payload(&notification);
        assert!(json.contains("\"completed\""));
    }
}
