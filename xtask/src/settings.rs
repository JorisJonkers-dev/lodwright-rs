// SPDX-License-Identifier: GPL-3.0-only
//! Repository settings that cannot live in a file (owner only), through the `gh` command.
//!
//! - `cargo xtask apply-rulesets [owner/repo]`: apply every ruleset in `.github/rulesets/*.json`, creating it when its
//!   name is absent and updating it in place otherwise. Idempotent.
//! - `cargo xtask setup-repo --audit [owner/repo]`: report which settings are present and which are missing; changes
//!   nothing.
//! - `cargo xtask setup-repo [owner/repo]`: apply what is missing (`DRY_RUN=1` prints the calls instead).
//!
//! Covers: merge methods and auto-merge, secret scanning and push protection, Dependabot alerts, private
//! vulnerability reporting, the workflow token, which actions may run (GitHub's, the organisation's and a short list,
//! pinned by SHA; `SHA_PINNING_REQUIRED=false` turns that requirement off), approval for outside contributors'
//! workflow runs, labels, and the branch and tag rulesets. It also lists who can write to the repository. Code scanning
//! is left out: it needs GitHub Advanced Security on a private repository.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

const DEFAULT_REPO: &str = "JorisJonkers-dev/lodwright-rs";

fn gh_json(args: &[&str]) -> Option<Value> {
    let output = Command::new("gh")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

fn gh_api_get(path: &str) -> Option<Value> {
    gh_json(&["api", path])
}

fn gh_api_get_all(path: &str) -> Option<Value> {
    gh_json(&["api", "--paginate", "--slurp", path]).map(|pages| match pages {
        Value::Array(pages) => Value::Array(
            pages
                .into_iter()
                .flat_map(|p| p.as_array().cloned().unwrap_or_default())
                .collect(),
        ),
        other => other,
    })
}

/// Run `gh api -X METHOD PATH [--input -]`; a failure is reported, not fatal.
fn call(dry: bool, method: &str, path: &str, body: Option<&str>) {
    if dry {
        println!("     + gh api -X {method} {path} {}", body.unwrap_or(""));
        return;
    }
    let mut command = Command::new("gh");
    command
        .args(["api", "-X", method, path])
        .stdout(Stdio::null());
    if body.is_some() {
        command.args(["--input", "-"]).stdin(Stdio::piped());
    }
    let ok = command.spawn().is_ok_and(|mut child| {
        if let (Some(body), Some(mut stdin)) = (body, child.stdin.take()) {
            let _ = stdin.write_all(body.as_bytes());
        }
        child.wait().is_ok_and(|s| s.success())
    });
    if !ok {
        eprintln!("     !! {method} {path} failed (feature unavailable, or no admin rights)");
    }
}

fn current_repo() -> Option<String> {
    gh_json(&["repo", "view", "--json", "nameWithOwner"]).and_then(|v| {
        v.get("nameWithOwner")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn rulesets_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/rulesets")
}

/// Apply every ruleset file to `repo`.
///
/// # Errors
/// A message when a ruleset file cannot be read.
pub fn apply_rulesets(repo: &str, dry: bool) -> Result<(), String> {
    let mut files: Vec<_> = std::fs::read_dir(rulesets_dir())
        .map_err(|e| format!("cannot list .github/rulesets: {e}"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    let existing = gh_api_get(&format!("/repos/{repo}/rulesets")).unwrap_or(Value::Array(vec![]));
    for file in files {
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        let ruleset: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        let name = ruleset
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = existing
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|r| r.get("id").and_then(Value::as_u64));
        if let Some(id) = id {
            println!("Updating ruleset '{name}' ({id}) on {repo}");
            call(
                dry,
                "PUT",
                &format!("/repos/{repo}/rulesets/{id}"),
                Some(&text),
            );
        } else {
            println!("Creating ruleset '{name}' on {repo}");
            call(dry, "POST", &format!("/repos/{repo}/rulesets"), Some(&text));
        }
    }
    Ok(())
}

struct Setup {
    repo: String,
    audit: bool,
    dry: bool,
    missing: u32,
}

impl Setup {
    fn ensure(&mut self, label: &str, present: bool, apply: impl FnOnce(&Self)) {
        if present {
            println!("  {label:<58} ok");
            return;
        }
        self.missing += 1;
        if self.audit {
            println!("  {label:<58} MISSING");
        } else {
            println!("  {label:<58} applying");
            apply(self);
        }
    }
}

fn field(value: Option<&Value>, path: &[&str]) -> Value {
    let mut current = value.cloned().unwrap_or(Value::Null);
    for key in path {
        current = current.get(key).cloned().unwrap_or(Value::Null);
    }
    current
}

fn ruleset_active(rulesets: Option<&Value>, name: &str) -> bool {
    rulesets.and_then(Value::as_array).is_some_and(|all| {
        all.iter()
            .filter(|r| {
                r.get("name").and_then(Value::as_str) == Some(name)
                    && r.get("enforcement").and_then(Value::as_str) == Some("active")
            })
            .count()
            == 1
    })
}

/// Audit the repository settings, or apply what is missing.
///
/// # Errors
/// A message when the repository cannot be read.
#[allow(clippy::too_many_lines)]
pub fn setup_repo(repo: &str, audit: bool, dry: bool) -> Result<(), String> {
    let repo_json = gh_api_get(&format!("repos/{repo}"))
        .ok_or_else(|| format!("cannot read {repo} (gh auth?)"))?;
    let sha_pinning = std::env::var("SHA_PINNING_REQUIRED").map_or(true, |v| v != "false");
    let mut s = Setup {
        repo: repo.to_owned(),
        audit,
        dry,
        missing: 0,
    };
    println!(
        "Repository settings of {repo} ({})",
        if audit { "audit" } else { "apply" }
    );

    let flag = |key: &str| repo_json.get(key).and_then(Value::as_bool).unwrap_or(false);
    s.ensure(
        "squash and rebase only, auto-merge, branch deleted after merge",
        flag("allow_squash_merge") && flag("allow_rebase_merge") && !flag("allow_merge_commit") && flag("allow_auto_merge") && flag("delete_branch_on_merge"),
        |s| call(s.dry, "PATCH", &format!("repos/{}", s.repo), Some(r#"{"allow_squash_merge":true,"allow_rebase_merge":true,"allow_merge_commit":false,"allow_auto_merge":true,"delete_branch_on_merge":true,"squash_merge_commit_title":"PR_TITLE","squash_merge_commit_message":"PR_BODY","has_wiki":false,"has_projects":false}"#)),
    );
    let security =
        |key: &str| field(Some(&repo_json), &["security_and_analysis", key, "status"]) == "enabled";
    s.ensure(
        "secret scanning and push protection",
        security("secret_scanning") && security("secret_scanning_push_protection"),
        |s| call(s.dry, "PATCH", &format!("repos/{}", s.repo), Some(r#"{"security_and_analysis":{"secret_scanning":{"status":"enabled"},"secret_scanning_push_protection":{"status":"enabled"}}}"#)),
    );
    let alerts = Command::new("gh")
        .args(["api", &format!("repos/{repo}/vulnerability-alerts")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|st| st.success());
    s.ensure(
        "Dependabot alerts (Renovate makes the update pull requests)",
        alerts,
        |s| {
            call(
                s.dry,
                "PUT",
                &format!("repos/{}/vulnerability-alerts", s.repo),
                None,
            );
        },
    );
    let pvr = field(
        gh_api_get(&format!("repos/{repo}/private-vulnerability-reporting")).as_ref(),
        &["enabled"],
    ) == true;
    s.ensure("private vulnerability reporting", pvr, |s| {
        call(
            s.dry,
            "PUT",
            &format!("repos/{}/private-vulnerability-reporting", s.repo),
            None,
        );
    });

    println!("Actions");
    let token = field(
        gh_api_get(&format!("repos/{repo}/actions/permissions/workflow")).as_ref(),
        &["default_workflow_permissions"],
    ) == "read";
    s.ensure("workflow token read-only by default", token, |s| {
        call(s.dry, "PUT", &format!("repos/{}/actions/permissions/workflow", s.repo), Some(r#"{"default_workflow_permissions":"read","can_approve_pull_request_reviews":false}"#));
    });
    let permissions = gh_api_get(&format!("repos/{repo}/actions/permissions"));
    let allowed = field(permissions.as_ref(), &["allowed_actions"]) == "selected"
        && field(permissions.as_ref(), &["sha_pinning_required"]) == sha_pinning;
    s.ensure("only GitHub's, the organisation's and three named actions, all pinned by SHA", allowed, |s| {
        let body = format!(r#"{{"enabled":true,"allowed_actions":"selected","sha_pinning_required":{sha_pinning}}}"#);
        call(s.dry, "PUT", &format!("repos/{}/actions/permissions", s.repo), Some(&body));
        call(s.dry, "PUT", &format!("repos/{}/actions/permissions/selected-actions", s.repo), Some(r#"{"github_owned_allowed":true,"verified_allowed":false,"patterns_allowed":["JorisJonkers-dev/*","jdx/mise-action@*","Swatinem/rust-cache@*","googleapis/release-please-action@*"]}"#));
    });
    let approval = field(
        gh_api_get(&format!(
            "repos/{repo}/actions/permissions/fork-pr-contributor-approval"
        ))
        .as_ref(),
        &["approval_policy"],
    ) == "all_external_contributors";
    s.ensure(
        "outside contributors' workflow runs need approval",
        approval,
        |s| {
            call(
                s.dry,
                "PUT",
                &format!(
                    "repos/{}/actions/permissions/fork-pr-contributor-approval",
                    s.repo
                ),
                Some(r#"{"approval_policy":"all_external_contributors"}"#),
            );
        },
    );

    println!("Labels and rulesets");
    let labels: Vec<String> = gh_api_get_all(&format!("repos/{repo}/labels"))
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect();
    let has = |name: &str| labels.iter().any(|l| l == name);
    s.ensure(
        "labels (hygiene exception, Renovate's)",
        has("allow-repository-hygiene-exception") && has("dependencies"),
        |s| {
            for (name, color, desc) in [
                (
                    "allow-repository-hygiene-exception",
                    "BFD4F2",
                    "Bypasses the repository hygiene guard",
                ),
                ("dependencies", "0366D6", "Dependency updates"),
                ("type: chore", "EDEDED", "Chore"),
                ("component: dependencies", "EDEDED", "Dependencies"),
            ] {
                if s.dry {
                    println!("     + gh label create '{name}'");
                } else {
                    let _ = Command::new("gh")
                        .args([
                            "label",
                            "create",
                            name,
                            "-R",
                            &s.repo,
                            "--color",
                            color,
                            "--description",
                            desc,
                            "--force",
                        ])
                        .stdout(Stdio::null())
                        .status();
                }
            }
        },
    );
    let rulesets = gh_api_get(&format!("repos/{repo}/rulesets"));
    let apply = |s: &Setup| {
        if let Err(e) = apply_rulesets(&s.repo, s.dry) {
            eprintln!("     !! {e}");
        }
    };
    s.ensure("ruleset Main: Pipeline Complete, pull requests only, squash, linear history, no force push", ruleset_active(rulesets.as_ref(), "Main"), apply);
    s.ensure(
        "ruleset Release tags: v* cannot be moved or deleted",
        ruleset_active(rulesets.as_ref(), "Release tags"),
        apply,
    );

    println!(
        "Who can change workflows (anyone with write access can read the repository's secrets)"
    );
    match gh_api_get(&format!("repos/{repo}/collaborators?affiliation=all"))
        .and_then(|v| v.as_array().cloned())
    {
        Some(people) => {
            let writers: Vec<_> = people
                .iter()
                .filter(|p| {
                    field(Some(p), &["permissions", "push"]) == true
                        || field(Some(p), &["permissions", "admin"]) == true
                })
                .collect();
            for p in &writers {
                let role = if field(Some(p), &["permissions", "admin"]) == true {
                    "admin"
                } else {
                    "write"
                };
                println!(
                    "  {}: {role}",
                    p.get("login").and_then(Value::as_str).unwrap_or("?")
                );
            }
            if writers.len() <= 1 {
                println!("  ok: only you");
            } else {
                println!(
                    "  !! more than one person can write; each of them can read the repository's secrets"
                );
            }
        }
        None => println!("  (cannot list: needs push access)"),
    }

    println!();
    if audit {
        println!(
            "{} item(s) missing. Run without --audit to apply them.",
            s.missing
        );
    } else {
        println!(
            "Applied what was missing ({}). Run with --audit to confirm.",
            s.missing
        );
    }
    println!(
        "\nStill by hand:\n  * Renovate (the organisation's app) onboards itself once renovate.json is on the default branch.\n  * release-please needs the RELEASE_APP_ID and RELEASE_APP_PRIVATE_KEY secrets to open release pull requests that\n    trigger the pipeline; without them it falls back to the workflow token, whose pull requests start no checks."
    );
    Ok(())
}

/// `cargo xtask setup-repo [--audit] [owner/repo]`.
#[must_use]
pub fn setup_main(args: &[String]) -> i32 {
    let audit = args.first().is_some_and(|a| a == "--audit");
    let rest = if audit { &args[1..] } else { args };
    let repo = rest.first().map_or(DEFAULT_REPO, String::as_str);
    let dry = std::env::var("DRY_RUN").is_ok_and(|v| v == "1");
    report(setup_repo(repo, audit, dry))
}

/// `cargo xtask apply-rulesets [owner/repo]`.
#[must_use]
pub fn rulesets_main(args: &[String]) -> i32 {
    let Some(repo) = args.first().cloned().or_else(current_repo) else {
        eprintln!("apply-rulesets: name the repository (owner/repo)");
        return 2;
    };
    let dry = std::env::var("DRY_RUN").is_ok_and(|v| v == "1");
    report(apply_rulesets(&repo, dry))
}

fn report(result: Result<(), String>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}
