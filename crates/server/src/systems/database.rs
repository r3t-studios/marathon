use bevy::prelude::*;

use crate::components::*;

/// System: Poll chat.db for new messages using Bevy's task system
pub fn poll_chat_db(_config: Res<AppConfig>, _db: Res<Database>) {
    // TODO: Use Bevy's AsyncComputeTaskPool to poll chat.db
    // This will replace the tokio::spawn chat poller
}
