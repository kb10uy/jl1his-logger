use std::{env, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-env-changed=GIT_COMMIT");
    if let Some(commit) = commit() {
        println!("cargo:rustc-env=JL1HIS_COMMIT={commit}");
    }
}

fn commit() -> Option<String> {
    if let Some(commit) = env::var("GIT_COMMIT").ok().filter(|c| !c.is_empty()) {
        return Some(commit);
    }
    let mut watched = vec![git(&["rev-parse", "--git-path", "HEAD"])?];
    if let Some(head) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.extend(git(&["rev-parse", "--git-path", &head]));
    }
    watched.extend(git(&["rev-parse", "--git-path", "packed-refs"]));
    for path in watched.iter().filter(|p| Path::new(p).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
    git(&["rev-parse", "HEAD"])
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    output.status.success().then(|| text.trim().to_owned())
}
