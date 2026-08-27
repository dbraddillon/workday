//! Source connector layer.
//!
//! `WorkSourceConnector` is the abstraction from the doc: auth, fetch, mapping,
//! pagination, normalization all live behind it. Jira is the only impl in v1;
//! `fake` provides deterministic sample data for dev and first-run.
//!
//! Everything a connector returns is already *normalized* into `crate::model`
//! types — callers never see raw source JSON.

pub mod fake;
pub mod github;
pub mod jira;

use crate::model::{ActivityEvent, Issue};

/// What a work source can produce. Async so real sources can do HTTP.
#[allow(async_fn_in_trait)]
pub trait WorkSourceConnector {
    /// Items matching the "in progress" query.
    async fn fetch_in_progress(&self) -> Result<Vec<Issue>, String>;
    /// Items matching the "recent" query, plus derived activity events.
    async fn fetch_recent(&self) -> Result<(Vec<Issue>, Vec<ActivityEvent>), String>;
}
