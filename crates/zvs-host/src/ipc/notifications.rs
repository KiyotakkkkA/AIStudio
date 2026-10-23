use std::sync::Arc;

use zvs_contract::{AppError, NotificationTargetDto};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server.handle(
        "notifications.clear",
        |services: Arc<Services>, input: NotificationTargetDto| async move {
            services.notifications.clear(input);
            Ok::<(), AppError>(())
        },
    );
}
