use soar_core::SoarResult;

use crate::SoarContext;

/// Sync all enabled repositories.
///
/// Emits `SoarEvent::SyncProgress` events through the context's event sink.
/// This is a convenience wrapper around `SoarContext::sync()`.
pub async fn sync_repos(ctx: &SoarContext) -> SoarResult<()> {
    ctx.sync().await
}
