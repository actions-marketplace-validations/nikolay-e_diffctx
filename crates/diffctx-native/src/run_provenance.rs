//! What a reader needs to reproduce or compare a run: the engine, the input
//! revisions, the effective configuration and its hash, the selection
//! parameters, and the resource limits that were in force. Deterministic by
//! construction — no timings live here, those stay in `latency`.

use std::path::Path;

use serde::Serialize;

use crate::effective_config::EffectiveConfigV1;

pub const SCHEMA: &str = "diffctx.provenance.v1";

#[derive(Serialize, Clone, Debug)]
pub struct Engine {
    pub name: &'static str,
    pub version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<&'static str>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Input {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_range: Option<String>,
    /// Resolved object ids, so the record still names the input after the
    /// ref moves. `head` is absent when the right side of the diff is the
    /// working tree — a bare `--diff`, `--diff HEAD~2`, a duration window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    pub working_tree: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Selection {
    pub budget_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_requested: Option<u32>,
    pub tau: f64,
    pub gate: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct ResourceLimits {
    pub timeout_secs: u64,
    pub max_file_bytes: usize,
    pub max_changed_file_bytes: usize,
    pub max_fragments_per_file: usize,
    pub max_out_edges_per_node: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct ProvenanceV1 {
    pub schema: &'static str,
    pub engine: Engine,
    pub input: Input,
    pub effective_config_hash: String,
    /// The full record in pack output; the compact locate schema carries the
    /// hash alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_config: Option<EffectiveConfigV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,
    pub resource_limits: ResourceLimits,
}

/// The heavy-phase half: everything known before selection runs. The
/// selection block is attached by whichever renderer spends the budget.
#[derive(Clone, Debug)]
pub struct RunProvenance {
    pub input: Input,
    pub effective_config: EffectiveConfigV1,
    pub timeout_secs: u64,
}

impl RunProvenance {
    pub fn new(
        root_dir: &Path,
        diff_range: Option<&str>,
        effective_config: EffectiveConfigV1,
        timeout_secs: u64,
    ) -> Self {
        let (base_rev, head_rev) = diff_range
            .map(crate::git::split_diff_range)
            .unwrap_or((None, None));
        // `git diff X` compares X with the working tree; only `A..B` names a
        // committed right side.
        let working_tree = head_rev.is_none();
        let head = head_rev
            .as_deref()
            .and_then(|rev| crate::git::rev_oid(root_dir, rev));
        let base = base_rev
            .as_deref()
            .or(diff_range)
            .or(Some("HEAD"))
            .and_then(|rev| crate::git::rev_oid(root_dir, rev));
        Self {
            input: Input {
                diff_range: diff_range.map(str::to_string),
                base,
                head,
                working_tree,
            },
            effective_config,
            timeout_secs,
        }
    }

    pub fn finish(&self, selection: Option<Selection>, include_config: bool) -> ProvenanceV1 {
        let limits = &*crate::config::limits::LIMITS;
        ProvenanceV1 {
            schema: SCHEMA,
            engine: Engine {
                name: "diffctx",
                version: env!("CARGO_PKG_VERSION"),
                build: option_env!("DIFFCTX_BUILD_SHA"),
            },
            input: self.input.clone(),
            effective_config_hash: self.effective_config.hash(),
            effective_config: include_config.then(|| self.effective_config.clone()),
            selection,
            resource_limits: ResourceLimits {
                timeout_secs: self.timeout_secs,
                max_file_bytes: limits.max_file_size,
                max_changed_file_bytes: limits.max_changed_file_size,
                max_fragments_per_file: limits.max_fragments,
                max_out_edges_per_node: self.effective_config.max_out_edges_per_node,
            },
        }
    }
}
