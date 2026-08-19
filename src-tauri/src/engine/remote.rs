//! Remote CI/CD + compliance status.
//!
//! Stubbed for the scaffold: returns representative pipeline + compliance badges so the
//! dashboard renders. A real implementation would poll the forge's API/webhooks on a
//! background worker and cache results in SQLite (`ci_status` table).

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CiStatus {
    pub pipeline: String,
    /// e.g. `success`, `failed`, `running`, `verified`.
    pub status: String,
    /// Short badge label surfaced in the UI, e.g. `passing`, `compliant`.
    pub badge: String,
    /// Unix epoch seconds of last update (0 in the scaffold).
    pub updated_at: f64,
}

/// Placeholder poll — swap for a real forge API call keyed off the repo's remote URL.
pub fn poll_ci(_path: &str) -> Vec<CiStatus> {
    vec![
        CiStatus {
            pipeline: "build".into(),
            status: "success".into(),
            badge: "passing".into(),
            updated_at: 0.0,
        },
        CiStatus {
            pipeline: "test".into(),
            status: "success".into(),
            badge: "passing".into(),
            updated_at: 0.0,
        },
        CiStatus {
            pipeline: "DCO / CLA".into(),
            status: "verified".into(),
            badge: "compliant".into(),
            updated_at: 0.0,
        },
    ]
}
