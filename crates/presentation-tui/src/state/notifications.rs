use std::collections::VecDeque;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub at: DateTime<Utc>,
    pub level: NotificationLevel,
    pub message: String,
}

/// Historial acotado de mensajes para el usuario (los más recientes al final).
#[derive(Debug, Clone, Default)]
pub struct Notifications {
    items: VecDeque<Notification>,
}

impl Notifications {
    /// Máximo de mensajes que se conservan.
    pub const CAPACITY: usize = 50;

    pub fn push(
        &mut self,
        at: DateTime<Utc>,
        level: NotificationLevel,
        message: impl Into<String>,
    ) {
        if self.items.len() == Self::CAPACITY {
            self.items.pop_front();
        }
        self.items.push_back(Notification {
            at,
            level,
            message: message.into(),
        });
    }

    pub fn latest(&self) -> Option<&Notification> {
        self.items.back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_most_recent_messages() {
        let mut notifications = Notifications::default();
        for index in 0..Notifications::CAPACITY + 5 {
            notifications.push(
                DateTime::UNIX_EPOCH,
                NotificationLevel::Info,
                format!("m{index}"),
            );
        }

        assert_eq!(notifications.items.len(), Notifications::CAPACITY);
        assert_eq!(
            notifications.latest().map(|n| n.message.as_str()),
            Some("m54")
        );
    }
}
