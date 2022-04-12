use std::path::Path;

fn main() {
    let hash = find_git_hash();
    let hash = hash.as_deref().unwrap_or("unknown");
    println!("cargo:rustc-env=GIT_HASH={}", hash);
}

fn find_git_hash() -> Option<String> {
    let repo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".git");
    let head_file = repo_path.join("HEAD");
    let head = std::fs::read_to_string(head_file.as_path()).ok()?;
    println!("cargo:rerun-if-changed={}", head_file.display());
    let mut hash = if let Some(reef) = head.trim().strip_prefix("ref: ") {
        let ref_path = repo_path.join(reef);
        let ref_head = std::fs::read_to_string(ref_path.as_path()).ok()?;
        println!("cargo:rerun-if-changed={}", ref_path.display());
        ref_head
    } else {
        head
    };

    hash.truncate(7);

    Some(hash)
}
