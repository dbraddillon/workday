//! GitHub review-queue connector.
//!
//! Answers one question: which PRs are open and plausibly waiting on me or my
//! team? Shells out to the locally-authed `gh` CLI rather than holding a token -
//! `gh auth` already has the credential, so nothing new goes in the Keychain and
//! there is no secret for this app to manage. Same "reuse what's installed,
//! degrade gracefully" approach as the `claude` CLI in `standup/summarizer.rs`.
//!
//! Why GraphQL and not `gh api search/issues`: the REST search endpoint is capped
//! at 30 requests/minute, which is too tight for a poll loop. GraphQL search costs
//! 1 point against 5000/hour and returns the review state and diff size in the
//! same round trip.
//!
//! Everything returned here is already normalized into `model::PullRequest`. The
//! GraphQL response shape does not leave this module.

use crate::model::PullRequest;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::process::Stdio;

/// GitHub's search query string has a length ceiling (~1000 chars in practice).
/// A roster of author qualifiers is the only part that grows with team size, so
/// it gets chunked across several queries rather than truncated. 40 authors is
/// ~700 chars of `author:login ` and leaves room for the fixed qualifiers.
const AUTHORS_PER_QUERY: usize = 40;

/// Review authors that are not people. A Copilot review still sets
/// `latestReviews`, which would make an unreviewed PR look attended-to.
const BOT_REVIEWER_MARKERS: &[&str] = &["copilot", "[bot]"];

/// PR authors that are not people. Dependency bumps are noise in a review queue.
const BOT_AUTHORS: &[&str] = &[
    "dependabot",
    "renovate",
    "snyk-bot",
    "github-actions",
    "copilot",
];

fn is_bot_author(login: &str) -> bool {
    let l = login.to_ascii_lowercase();
    l.ends_with("[bot]") || BOT_AUTHORS.iter().any(|b| l == *b || l.starts_with(b))
}

fn is_bot_reviewer(login: &str) -> bool {
    let l = login.to_ascii_lowercase();
    BOT_REVIEWER_MARKERS.iter().any(|m| l.contains(m))
}

/// Best-effort check that the `gh` CLI exists and is authenticated. Gates the
/// Reviews tab in the UI the way `claude_cli_available` gates the AI-polish
/// toggle - a teammate without `gh` is never shown an empty tab with no
/// explanation. `gh auth status` exits non-zero when there is no valid token.
pub async fn gh_available() -> bool {
    tokio::process::Command::new("gh")
        .args(["auth", "status"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

// --------------------------- GraphQL response shapes -------------------------
// Private to this module: the seam is `model::PullRequest`.

#[derive(Deserialize)]
struct GqlEnvelope {
    data: Option<GqlData>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Deserialize)]
struct GqlData {
    #[serde(flatten)]
    searches: BTreeMap<String, GqlSearch>,
}

#[derive(Deserialize)]
struct GqlSearch {
    #[serde(default)]
    nodes: Vec<Option<GqlNode>>,
}

#[derive(Deserialize)]
struct GqlNode {
    number: Option<i64>,
    title: Option<String>,
    url: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    author: Option<GqlActor>,
    repository: Option<GqlRepo>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
    additions: Option<i64>,
    deletions: Option<i64>,
    #[serde(rename = "changedFiles")]
    changed_files: Option<i64>,
    #[serde(rename = "latestReviews")]
    latest_reviews: Option<GqlReviews>,
}

#[derive(Deserialize)]
struct GqlActor {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GqlRepo {
    name: Option<String>,
}

#[derive(Deserialize)]
struct GqlReviews {
    #[serde(default)]
    nodes: Vec<Option<GqlReview>>,
}

#[derive(Deserialize)]
struct GqlReview {
    author: Option<GqlActor>,
}

// Shapes for `fetch_submitted_reviews`: a single unaliased search whose nodes
// carry the viewer's own reviews.

#[derive(Deserialize)]
struct ReviewEnvelope {
    data: Option<ReviewData>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct ReviewData {
    search: ReviewSearch,
}

#[derive(Deserialize)]
struct ReviewSearch {
    #[serde(default)]
    nodes: Vec<Option<ReviewNode>>,
}

#[derive(Deserialize)]
struct ReviewNode {
    number: Option<i64>,
    title: Option<String>,
    url: Option<String>,
    /// PR state: OPEN / MERGED / CLOSED.
    state: Option<String>,
    repository: Option<GqlRepo>,
    author: Option<GqlActor>,
    reviews: Option<SubmittedReviewNodes>,
}

#[derive(Deserialize)]
struct SubmittedReviewNodes {
    #[serde(default)]
    nodes: Vec<Option<SubmittedReviewNode>>,
}

#[derive(Deserialize)]
struct SubmittedReviewNode {
    #[serde(rename = "submittedAt")]
    submitted_at: Option<String>,
    state: Option<String>,
}

/// The PR fields every search alias selects.
const PR_FRAGMENT: &str = r#"
fragment PR on PullRequest {
  number title url createdAt updatedAt
  author { login }
  repository { name }
  reviewDecision
  additions deletions changedFiles
  latestReviews(first: 20) { nodes { author { login } } }
}
"#;

/// Config for one review-queue fetch.
pub struct GithubConnector {
    /// Org login, e.g. "healthsparq".
    pub org: String,
    /// The viewer's own login, so their PRs are excluded from the queue.
    pub login: String,
    /// Team slugs whose review requests count.
    pub teams: Vec<String>,
    /// Age window in days, applied to PR creation date.
    pub window_days: i64,
    /// Cap on rows returned. The team can open 50+ PRs a week, so an unbounded
    /// list is a real possibility on a week where reviews stall.
    pub max_results: usize,
    /// Include PRs authored by team members, not just PRs where a team was
    /// tagged for review.
    pub include_team_authored: bool,
}

impl GithubConnector {
    /// Fetch the review queue. Returns rows plus the pre-cap total, so the UI can
    /// say "showing N of M" instead of silently truncating.
    pub async fn fetch_review_queue(&self) -> Result<(Vec<PullRequest>, usize), String> {
        if self.org.trim().is_empty() {
            return Err("GitHub org is not set. Add one in Settings.".into());
        }
        if self.login.trim().is_empty() {
            return Err("GitHub login is not set. Add one in Settings.".into());
        }

        // Team roster drives the "authored by a teammate" side of the union.
        // A failure here is not fatal: the team-tagged and direct sides still
        // work, so the tab degrades rather than going blank.
        let mut roster = Vec::new();
        if self.include_team_authored {
            for team in &self.teams {
                match self.fetch_team_members(team).await {
                    Ok(m) => roster.extend(m),
                    Err(_) => continue,
                }
            }
            roster.sort();
            roster.dedup();
            roster.retain(|m| !m.eq_ignore_ascii_case(&self.login));
        }

        let since = crate::connector::github::window_start(self.window_days);
        let mut by_pr: BTreeMap<(String, i64), PullRequest> = BTreeMap::new();

        // --- Windowed sides: team-tagged, and teammate-authored. -------------
        let base = format!(
            "is:open is:pr draft:false org:{} created:>={} -review:approved",
            self.org, since
        );

        if !self.teams.is_empty() {
            let teams_q = self
                .teams
                .iter()
                .map(|t| format!("team-review-requested:{}/{}", self.org, t))
                .collect::<Vec<_>>()
                .join(" ");
            let q = format!("{base} {teams_q}");
            for pr in self.search(&[("team", &q)]).await? {
                merge(&mut by_pr, pr, "team");
            }
        }

        for chunk in roster.chunks(AUTHORS_PER_QUERY) {
            let authors_q = chunk
                .iter()
                .map(|a| format!("author:{a}"))
                .collect::<Vec<_>>()
                .join(" ");
            let q = format!("{base} {authors_q}");
            for pr in self.search(&[("authored", &q)]).await? {
                merge(&mut by_pr, pr, "authored");
            }
        }

        // --- Trump sides: no age window, no approval filter. ----------------
        // Requested of me personally, or assigned to me. These outrank the
        // window entirely - an old PR waiting on me by name is exactly the thing
        // that must not age out of the list.
        let direct_q = format!(
            "is:open is:pr org:{} user-review-requested:{}",
            self.org, self.login
        );
        let assigned_q = format!("is:open is:pr org:{} assignee:{}", self.org, self.login);
        for pr in self
            .search(&[("direct", &direct_q), ("assigned", &assigned_q)])
            .await?
        {
            // `search` tags rows with their alias, so read it back off the row.
            let reason = pr.reasons.first().cloned().unwrap_or_default();
            merge(&mut by_pr, pr, &reason);
        }

        // Drop the viewer's own PRs and bot-authored noise - unless a trump
        // reason pulled them in, in which case the explicit signal wins.
        let mut rows: Vec<PullRequest> = by_pr
            .into_values()
            .filter(|p| p.is_direct || (!p.author.eq_ignore_ascii_case(&self.login) && !is_bot_author(&p.author)))
            .collect();

        // Direct/assigned first, then newest. `is_direct` is the trump rule made
        // visible in the ordering, not just in membership.
        rows.sort_by(|a, b| {
            b.is_direct
                .cmp(&a.is_direct)
                .then_with(|| b.created_at.cmp(&a.created_at))
        });

        let total = rows.len();
        rows.truncate(self.max_results);
        Ok((rows, total))
    }

    /// Reviews the user actually submitted since `since_days` ago.
    ///
    /// Two timestamps are in play and conflating them is the trap here. GitHub's
    /// `reviewed-by:` search qualifier can only be date-filtered on the PR's
    /// `updated:`, but what we want is when the *review* was submitted. So the
    /// search window is deliberately widened (a review's PR may not have been
    /// touched since) and the real filter happens on each review's `submittedAt`
    /// after the fetch.
    pub async fn fetch_submitted_reviews(
        &self,
        since_days: i64,
    ) -> Result<Vec<crate::model::SubmittedReview>, String> {
        if self.org.trim().is_empty() || self.login.trim().is_empty() {
            return Err("GitHub org and login are required.".into());
        }
        // Widen the search window so a review on a since-untouched PR is still
        // found, then filter precisely on submittedAt below.
        let search_since = window_start(since_days.max(1) * 3 + 7);
        let cutoff = chrono::Utc::now() - chrono::Duration::days(since_days.max(0));

        let query = format!(
            r#"query($q: String!) {{
  search(query: $q, type: ISSUE, first: 100) {{
    nodes {{ ... on PullRequest {{
      number title url state
      repository {{ name }}
      author {{ login }}
      reviews(first: 20, author: "{}") {{ nodes {{ submittedAt state }} }}
    }} }}
  }}
}}"#,
            self.login
        );
        let q = format!(
            "is:pr reviewed-by:{} org:{} updated:>={}",
            self.login, self.org, search_since
        );

        let out = tokio::process::Command::new("gh")
            .args([
                "api",
                "graphql",
                "-f",
                &format!("query={query}"),
                "-f",
                &format!("q={q}"),
            ])
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| format!("could not launch `gh`: {e}"))?;

        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() && stdout.trim().is_empty() {
            return Err(format!(
                "gh api graphql failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }

        let env: ReviewEnvelope = serde_json::from_str(&stdout)
            .map_err(|e| format!("could not parse gh GraphQL response: {e}"))?;
        if !env.errors.is_empty() {
            let msgs: Vec<&str> = env.errors.iter().map(|e| e.message.as_str()).collect();
            return Err(format!("GitHub GraphQL: {}", msgs.join("; ")));
        }

        let mut rows = Vec::new();
        for node in env.data.into_iter().flat_map(|d| d.search.nodes).flatten() {
            let (Some(repo), Some(number)) = (
                node.repository.as_ref().and_then(|r| r.name.clone()),
                node.number,
            ) else {
                continue;
            };
            for rev in node.reviews.iter().flat_map(|r| r.nodes.iter()).flatten() {
                let Some(submitted) = rev.submitted_at.clone() else {
                    continue;
                };
                // The precise filter: the review's own submission time.
                match chrono::DateTime::parse_from_rfc3339(&submitted) {
                    Ok(ts) if ts.with_timezone(&chrono::Utc) >= cutoff => {}
                    _ => continue,
                }
                rows.push(crate::model::SubmittedReview {
                    repo: repo.clone(),
                    number,
                    title: node.title.clone().unwrap_or_default(),
                    url: node.url.clone().unwrap_or_default(),
                    author: node
                        .author
                        .as_ref()
                        .and_then(|a| a.login.clone())
                        .unwrap_or_else(|| "unknown".into()),
                    submitted_at: submitted,
                    state: rev.state.clone().unwrap_or_default(),
                    pr_state: node.state.clone().unwrap_or_default(),
                });
            }
        }
        // Newest first.
        rows.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
        Ok(rows)
    }

    /// Team members via the REST API. Paginated by `gh --paginate`.
    async fn fetch_team_members(&self, team: &str) -> Result<Vec<String>, String> {
        let path = format!("orgs/{}/teams/{}/members", self.org, team);
        let out = tokio::process::Command::new("gh")
            .args(["api", "--paginate", &path, "--jq", ".[].login"])
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| format!("could not launch `gh`: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "gh api {path} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    /// Run one GraphQL request holding one search alias per `(alias, query)`
    /// pair. Each row is tagged with its alias in `reasons`.
    async fn search(&self, queries: &[(&str, &str)]) -> Result<Vec<PullRequest>, String> {
        if queries.is_empty() {
            return Ok(Vec::new());
        }

        let params: Vec<String> = (0..queries.len()).map(|i| format!("$q{i}: String!")).collect();
        let aliases: Vec<String> = queries
            .iter()
            .enumerate()
            .map(|(i, (alias, _))| {
                format!("  {alias}: search(query: $q{i}, type: ISSUE, first: 60) {{ nodes {{ ...PR }} }}")
            })
            .collect();
        let query = format!(
            "query({}) {{\n{}\n}}\n{}",
            params.join(", "),
            aliases.join("\n"),
            PR_FRAGMENT
        );

        let mut cmd = tokio::process::Command::new("gh");
        cmd.args(["api", "graphql", "-f", &format!("query={query}")]);
        for (i, (_, q)) in queries.iter().enumerate() {
            cmd.arg("-f").arg(format!("q{i}={q}"));
        }
        let out = cmd
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| format!("could not launch `gh`: {e}"))?;

        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() && stdout.trim().is_empty() {
            return Err(format!(
                "gh api graphql failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }

        let env: GqlEnvelope = serde_json::from_str(&stdout)
            .map_err(|e| format!("could not parse gh GraphQL response: {e}"))?;
        if !env.errors.is_empty() {
            let msgs: Vec<&str> = env.errors.iter().map(|e| e.message.as_str()).collect();
            return Err(format!("GitHub GraphQL: {}", msgs.join("; ")));
        }
        let data = env.data.ok_or("GitHub GraphQL returned no data")?;

        let mut rows = Vec::new();
        for (alias, search) in &data.searches {
            for node in search.nodes.iter().flatten() {
                if let Some(pr) = normalize(node, alias) {
                    rows.push(pr);
                }
            }
        }
        Ok(rows)
    }
}

/// Normalize one GraphQL node. Returns `None` for a node missing the fields that
/// identify it - a search can include non-PR nodes, which come back empty.
fn normalize(n: &GqlNode, reason: &str) -> Option<PullRequest> {
    let repo = n.repository.as_ref()?.name.clone()?;
    let number = n.number?;
    let human_reviewers = n
        .latest_reviews
        .as_ref()
        .map(|r| {
            r.nodes
                .iter()
                .flatten()
                .filter_map(|rev| rev.author.as_ref()?.login.clone())
                .filter(|l| !is_bot_reviewer(l))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(PullRequest {
        repo,
        number,
        title: n.title.clone().unwrap_or_default(),
        url: n.url.clone().unwrap_or_default(),
        author: n
            .author
            .as_ref()
            .and_then(|a| a.login.clone())
            .unwrap_or_else(|| "unknown".into()),
        created_at: n.created_at.clone().unwrap_or_default(),
        updated_at: n.updated_at.clone().unwrap_or_default(),
        review_decision: n
            .review_decision
            .clone()
            .unwrap_or_else(|| "NONE".to_string()),
        additions: n.additions.unwrap_or(0),
        deletions: n.deletions.unwrap_or(0),
        changed_files: n.changed_files.unwrap_or(0),
        human_reviewers,
        reasons: vec![reason.to_string()],
        is_direct: matches!(reason, "direct" | "assigned"),
        reviewed_at: None,
    })
}

/// Fold a row into the dedup map, unioning the reason rather than overwriting -
/// a PR that is both team-tagged and teammate-authored should show both.
fn merge(map: &mut BTreeMap<(String, i64), PullRequest>, pr: PullRequest, reason: &str) {
    let key = (pr.repo.clone(), pr.number);
    match map.get_mut(&key) {
        Some(existing) => {
            if !reason.is_empty() && !existing.reasons.iter().any(|r| r == reason) {
                existing.reasons.push(reason.to_string());
            }
            existing.is_direct |= matches!(reason, "direct" | "assigned");
        }
        None => {
            map.insert(key, pr);
        }
    }
}

/// `YYYY-MM-DD` for `window_days` ago, the form GitHub's `created:>=` wants.
fn window_start(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days.max(0)))
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copilot_is_not_a_human_reviewer() {
        assert!(is_bot_reviewer("copilot-pull-request-reviewer"));
        assert!(is_bot_reviewer("some-app[bot]"));
        assert!(!is_bot_reviewer("zachheath"));
    }

    #[test]
    fn dependency_bots_are_filtered_as_authors() {
        assert!(is_bot_author("dependabot"));
        assert!(is_bot_author("dependabot[bot]"));
        assert!(is_bot_author("renovate"));
        assert!(!is_bot_author("Copeland-Kyle"));
    }

    #[test]
    fn merge_unions_reasons_instead_of_overwriting() {
        let mut map = BTreeMap::new();
        let pr = PullRequest {
            repo: "r".into(),
            number: 1,
            title: "t".into(),
            url: "u".into(),
            author: "a".into(),
            created_at: "2026-08-25T00:00:00Z".into(),
            updated_at: "2026-08-26T00:00:00Z".into(),
            review_decision: "REVIEW_REQUIRED".into(),
            additions: 1,
            deletions: 0,
            changed_files: 1,
            human_reviewers: vec![],
            reasons: vec!["team".into()],
            is_direct: false,
            reviewed_at: None,
        };
        merge(&mut map, pr.clone(), "team");
        merge(&mut map, pr, "authored");
        let got = map.values().next().unwrap();
        assert_eq!(got.reasons, vec!["team".to_string(), "authored".to_string()]);
    }

    /// Hits live GitHub through the local `gh` CLI, so it is ignored by default
    /// (needs auth, network, and org membership). Run deliberately:
    ///   cargo test live_review_queue -- --ignored --nocapture
    /// Override the target with GH_ORG / GH_LOGIN / GH_TEAMS.
    #[tokio::test]
    #[ignore]
    async fn live_review_queue() {
        assert!(gh_available().await, "`gh` is not authenticated");
        let c = GithubConnector {
            org: std::env::var("GH_ORG").unwrap_or_else(|_| "healthsparq".into()),
            login: std::env::var("GH_LOGIN").unwrap_or_else(|_| "dbraddillon".into()),
            teams: std::env::var("GH_TEAMS")
                .unwrap_or_else(|_| "health-plan-apps-and-services".into())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            window_days: 7,
            max_results: 40,
            include_team_authored: true,
        };
        let (prs, total) = c.fetch_review_queue().await.expect("fetch failed");
        println!("total {total}, returned {}", prs.len());
        for p in &prs {
            println!(
                "  {}#{} {} +{}/-{} {:?} humans:{:?} direct:{}",
                p.repo, p.number, p.author, p.additions, p.deletions,
                p.reasons, p.human_reviewers, p.is_direct
            );
        }
        // Invariants that must hold regardless of what the org looks like today.
        for p in &prs {
            assert!(!p.repo.is_empty(), "repo missing");
            assert!(p.number > 0, "number missing");
            assert!(p.url.starts_with("https://"), "url missing: {:?}", p.url);
            assert!(!p.reasons.is_empty(), "row has no reason");
            assert_ne!(
                p.review_decision, "APPROVED",
                "approved PR leaked into the queue: {}#{}",
                p.repo, p.number
            );
            assert!(
                !p.author.eq_ignore_ascii_case(&c.login),
                "own PR leaked in: {}#{}",
                p.repo,
                p.number
            );
            assert!(
                !p.human_reviewers.iter().any(|r| is_bot_reviewer(r)),
                "bot reviewer counted as human on {}#{}",
                p.repo,
                p.number
            );
        }
        // Trump rows must sort ahead of windowed ones.
        let first_non_direct = prs.iter().position(|p| !p.is_direct);
        if let (Some(i), Some(j)) = (
            first_non_direct,
            prs.iter().rposition(|p| p.is_direct),
        ) {
            assert!(i > j, "direct/assigned PRs must sort first");
        }
    }

    /// Live, same conditions as `live_review_queue`:
    ///   cargo test live_submitted_reviews -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_submitted_reviews() {
        assert!(gh_available().await, "`gh` is not authenticated");
        let c = GithubConnector {
            org: std::env::var("GH_ORG").unwrap_or_else(|_| "healthsparq".into()),
            login: std::env::var("GH_LOGIN").unwrap_or_else(|_| "dbraddillon".into()),
            teams: vec![],
            window_days: 7,
            max_results: 40,
            include_team_authored: false,
        };
        let reviews = c.fetch_submitted_reviews(7).await.expect("fetch failed");
        println!("{} reviews in the last 7 days", reviews.len());
        // Per-day tally: the number that motivated this path.
        let mut by_day: std::collections::BTreeMap<String, usize> = Default::default();
        for r in &reviews {
            *by_day.entry(r.submitted_at[..10].to_string()).or_default() += 1;
            println!(
                "  {} {}#{} by {} [{}] pr:{}",
                r.submitted_at, r.repo, r.number, r.author, r.state, r.pr_state
            );
        }
        for (day, n) in &by_day {
            println!("  {day}: {n}");
        }

        let cutoff = window_start(7);
        for r in &reviews {
            assert!(!r.repo.is_empty(), "repo missing");
            assert!(r.number > 0, "number missing");
            assert!(r.url.starts_with("https://"), "url missing: {:?}", r.url);
            // The whole point of filtering on submittedAt: nothing older leaks in.
            assert!(
                r.submitted_at[..10] >= *cutoff,
                "review outside the window: {} on {}#{}",
                r.submitted_at,
                r.repo,
                r.number
            );
        }
        // Newest first.
        for pair in reviews.windows(2) {
            assert!(
                pair[0].submitted_at >= pair[1].submitted_at,
                "not sorted newest-first"
            );
        }
    }

    #[test]
    fn direct_reason_sets_the_trump_flag() {
        let node = GqlNode {
            number: Some(7),
            title: Some("t".into()),
            url: Some("u".into()),
            created_at: Some("2026-01-01T00:00:00Z".into()),
            updated_at: Some("2026-01-02T00:00:00Z".into()),
            author: Some(GqlActor { login: Some("x".into()) }),
            repository: Some(GqlRepo { name: Some("r".into()) }),
            review_decision: Some("REVIEW_REQUIRED".into()),
            additions: Some(1),
            deletions: Some(1),
            changed_files: Some(1),
            latest_reviews: None,
        };
        assert!(normalize(&node, "direct").unwrap().is_direct);
        assert!(normalize(&node, "assigned").unwrap().is_direct);
        assert!(!normalize(&node, "team").unwrap().is_direct);
    }
}
