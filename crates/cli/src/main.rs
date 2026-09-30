mod config;
use inheriteame_core::{apply_edits, parser::inspect};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    protocol_version: u32,
    project_dir: PathBuf,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    check: bool,
    #[serde(default)]
    allow_unstaged: bool,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: u32,
    selected_files: usize,
    reviewed_files: usize,
    changed_files: usize,
    excluded_files: usize,
    proposed_edits: usize,
    errors: usize,
    diagnostics: Vec<String>,
    files: Vec<FileReport>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileReport {
    path: String,
    status: String,
    reason: String,
    diff: Option<String>,
}

fn main() {
    let mut report = Report {
        schema_version: 1,
        ..Report::default()
    };
    let run = (|| {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| e.to_string())?;
        let request: Request =
            serde_json::from_str(&input).map_err(|e| format!("Invalid request: {e}"))?;
        if request.protocol_version != 1 {
            return Err("Unsupported protocol version".into());
        }
        if request.check && request.dry_run || request.all && request.allow_unstaged {
            return Err("Incompatible flags".into());
        }
        process(&request, &mut report)
    })();
    let exit_code = match run {
        Ok(code) => code,
        Err(message) => {
            report.errors += 1;
            report.diagnostics.push(message);
            2
        }
    };
    println!(
        "{}",
        serde_json::json!({"protocolVersion":1,"event":"finished","exitCode":exit_code,"result":report})
    );
    std::process::exit(exit_code);
}
fn process(request: &Request, report: &mut Report) -> Result<i32, String> {
    let project = find_project(&request.project_dir)?;
    let config = config::Config::load(&project)?;
    let packages = package_directories(&project)?;
    let mut files = if request.all {
        walk_packages(&packages)?
    } else {
        staged_files(&project, &packages)?
    };
    files.sort();
    files.dedup();
    report.selected_files = files.len();
    // Plan every file first, so configuration, read and partial-stage failures precede writes.
    let mut plans = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(&project)
            .map_err(|_| "Path escaped project")?
            .to_string_lossy()
            .replace('\\', "/");
        let mut item = FileReport {
            path: relative.clone(),
            status: "unchanged".into(),
            reason: String::new(),
            diff: None,
        };
        if config.files.is_match(&relative) {
            item.status = "excluded".into();
            item.reason = "excludeFiles".into();
        } else {
            let bytes = fs::read(&path).map_err(|e| format!("Cannot read {relative}: {e}"))?;
            match String::from_utf8(bytes) {
                Err(_) => {
                    item.status = "error".into();
                    item.reason = "INVALID_UTF8".into();
                }
                Ok(source) => {
                    let inspected = inspect(&source);
                    if inspected.ignored {
                        item.status = "excluded".into();
                        item.reason = "header-marker".into();
                    } else if inspected
                        .class_name
                        .as_ref()
                        .is_some_and(|name| config.classes.is_match(name))
                    {
                        item.status = "excluded".into();
                        item.reason = "ignoreClassPatterns".into();
                    } else {
                        if !request.all && !request.allow_unstaged {
                            reject_unstaged(&project, &path)?;
                        }
                        if !inspected.analysis.diagnostics.is_empty() {
                            item.status = "error".into();
                            item.reason = "PARSE_ERROR".into();
                        } else if inspected.analysis.edits.is_empty() {
                            item.reason = if inspected.class_name.is_some() {
                                "explicit-sharing"
                            } else {
                                "no-top-level-class"
                            }
                            .into();
                        } else {
                            let fixed = apply_edits(&source, &inspected.analysis.edits)
                                .map_err(str::to_owned)?;
                            // Check the resulting syntax before accepting a rewrite.
                            if !inspect(&fixed).analysis.diagnostics.is_empty() {
                                return Err(format!("Rewrite produced invalid Apex: {relative}"));
                            }
                            report.proposed_edits += inspected.analysis.edits.len();
                            item.status = "pending".into();
                            item.reason = "missing-sharing".into();
                            item.diff = Some(
                                similar::TextDiff::from_lines(&source, &fixed)
                                    .unified_diff()
                                    .header(&format!("a/{relative}"), &format!("b/{relative}"))
                                    .to_string(),
                            );
                            plans.push((path, source, fixed, report.files.len()));
                        }
                    }
                }
            }
        }
        if item.status == "excluded" {
            report.excluded_files += 1;
        } else {
            report.reviewed_files += 1;
        }
        if item.status == "error" {
            report.errors += 1;
            report
                .diagnostics
                .push(format!("{}: {}", item.path, item.reason));
        }
        report.files.push(item);
    }
    if !request.dry_run && !request.check {
        for (path, original, fixed, index) in plans {
            if fs::read(&path).map_err(|e| e.to_string())? != original.as_bytes() {
                return Err(format!("File changed during analysis: {}", path.display()));
            }
            fs::write(&path, fixed).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
            report.files[index].status = "modified".into();
            report.changed_files += 1;
        }
    }
    Ok(i32::from(
        report.errors > 0 || request.check && report.proposed_edits > 0,
    ))
}
fn find_project(start: &Path) -> Result<PathBuf, String> {
    let mut current = start.canonicalize().map_err(|e| e.to_string())?;
    loop {
        if current.join("sfdx-project.json").is_file() {
            return Ok(current);
        }
        if !current.pop() {
            return Err("Could not find sfdx-project.json".into());
        }
    }
}
fn package_directories(project: &Path) -> Result<Vec<PathBuf>, String> {
    let text = fs::read_to_string(project.join("sfdx-project.json")).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    value
        .get("packageDirectories")
        .and_then(|v| v.as_array())
        .ok_or("Missing packageDirectories")?
        .iter()
        .map(|entry| {
            let relative = entry
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("Invalid package directory")?;
            let path = project
                .join(relative)
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !path.starts_with(project) || !path.is_dir() {
                return Err("Package directory must be inside the project".into());
            }
            Ok(path)
        })
        .collect()
}
fn git(project: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    Ok(output.stdout)
}
fn staged_files(project: &Path, packages: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let root = git(project, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(String::from_utf8(root).map_err(|e| e.to_string())?.trim());
    let names = git(
        project,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ],
    )?;
    let mut files = vec![];
    for name in names.split(|b| *b == 0).filter(|n| !n.is_empty()) {
        let path = root.join(std::str::from_utf8(name).map_err(|e| e.to_string())?);
        if path.extension().is_some_and(|e| e == "cls") && path.is_file() && !path.is_symlink() {
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            if packages.iter().any(|p| path.starts_with(p)) {
                files.push(path);
            }
        }
    }
    Ok(files)
}
fn walk_packages(packages: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = vec![];
    for path in packages {
        walk(path, &mut files)?;
    }
    Ok(files)
}
fn walk(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() && !matches!(entry.file_name().to_str(), Some(".git" | "node_modules")) {
            walk(&entry.path(), files)?;
        } else if kind.is_file() && entry.path().extension().is_some_and(|e| e == "cls") {
            files.push(entry.path());
        }
    }
    Ok(())
}
fn reject_unstaged(project: &Path, path: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["diff", "--quiet", "--"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    match output.status.code() {
        Some(0) => Ok(()),
        Some(1) => Err(format!(
            "PARTIAL_STAGE: {} has unstaged changes; use --allow-unstaged",
            path.display()
        )),
        _ => Err(String::from_utf8_lossy(&output.stderr).into()),
    }
}
