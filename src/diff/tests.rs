use super::*;

// `git diff --cached -z --no-renames --name-status` / `--numstat` formats.
const NAME_STATUS: &str = "M\0src/diff.rs\0A\0Cargo.lock\0A\0assets/logo.png\0D\0old file.txt\0";
const NUMSTAT: &str =
    "10\t2\tsrc/diff.rs\x00300\t0\tCargo.lock\x00-\t-\tassets/logo.png\x000\t1\told file.txt\x00";
const PATCH: &str = "\
diff --git a/src/diff.rs b/src/diff.rs
index 111..222 100644
--- a/src/diff.rs
+++ b/src/diff.rs
@@ -1,2 +1,10 @@
+fn parse() {}
diff --git a/Cargo.lock b/Cargo.lock
new file mode 100644
index 000..333
--- /dev/null
+++ b/Cargo.lock
@@ -0,0 +1,300 @@
+[[package]]
diff --git a/assets/logo.png b/assets/logo.png
new file mode 100644
index 000..444
Binary files /dev/null and b/assets/logo.png differ
diff --git a/old file.txt b/old file.txt
deleted file mode 100644
index 555..000
--- a/old file.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
";

fn parsed() -> Vec<FileChange> {
    parse(NAME_STATUS, NUMSTAT, PATCH)
}

#[test]
fn parses_status_counts_and_binary() {
    let files = parsed();
    assert_eq!(files.len(), 4);
    assert_eq!(files[0].path, "src/diff.rs");
    assert_eq!(files[0].status, Status::Modified);
    assert_eq!((files[0].added, files[0].removed), (10, 2));
    assert!(!files[0].binary);
    assert_eq!(files[1].status, Status::Added);
    assert!(files[2].binary);
    assert_eq!(files[3].path, "old file.txt");
    assert_eq!(files[3].status, Status::Deleted);
}

#[test]
fn attaches_patch_by_path_and_drops_index_line() {
    let files = parsed();
    assert!(
        files[0]
            .patch
            .starts_with("diff --git a/src/diff.rs b/src/diff.rs")
    );
    assert!(files[0].patch.contains("+fn parse() {}"));
    assert!(!files[0].patch.contains("index 111..222"));
    assert!(!files[0].patch.contains("Cargo.lock"));
    assert!(files[3].patch.contains("-bye"));
}

#[test]
fn typechange_keeps_all_blocks_for_the_file() {
    // file -> symlink: one name-status entry, two patch blocks.
    let ns = "T\0x\0M\0y.rs\0";
    let num = "0\t1\tx\x001\t0\tx\x001\t1\ty.rs\0";
    let patch = "\
diff --git a/x b/x
deleted file mode 100644
-old
diff --git a/x b/x
new file mode 120000
+target
diff --git a/y.rs b/y.rs
+v2
";
    let files = parse(ns, num, patch);
    assert_eq!(files.len(), 2);
    assert!(files[0].patch.contains("-old"));
    assert!(files[0].patch.contains("+target"));
    assert!(files[1].patch.contains("+v2"));
    assert!(!files[1].patch.contains("target"));
}

#[test]
fn unmerged_status_is_detected() {
    let files = parse("U\0c.txt\0", "", "");
    assert_eq!(files[0].status, Status::Unmerged);
}

#[test]
fn skip_reason_classifies_noise() {
    let files = parsed();
    assert_eq!(skip_reason(&files[0]), None);
    assert_eq!(skip_reason(&files[1]), Some("lockfile"));
    assert_eq!(skip_reason(&files[2]), Some("binary"));
    let min = FileChange::new("web/app.min.js", Status::Modified, 1, 1);
    assert_eq!(skip_reason(&min), Some("generated"));
    let dist = FileChange::new("dist/index.js", Status::Modified, 1, 1);
    assert_eq!(skip_reason(&dist), Some("generated"));
    let nested_lock = FileChange::new("web/package-lock.json", Status::Modified, 1, 1);
    assert_eq!(skip_reason(&nested_lock), Some("lockfile"));
}

#[test]
fn render_lists_every_file_but_only_includes_useful_patches() {
    let out = render(&parsed(), Limits::default());
    assert!(out.contains("M src/diff.rs (+10 -2)"));
    assert!(out.contains("A Cargo.lock (+300 -0) [diff omitted: lockfile]"));
    assert!(out.contains("A assets/logo.png (binary) [diff omitted: binary]"));
    assert!(out.contains("D old file.txt (+0 -1)"));
    assert!(out.contains("+fn parse() {}"));
    assert!(!out.contains("[[package]]"));
}

#[test]
fn render_truncates_long_file_patch() {
    let mut f = FileChange::new("src/big.rs", Status::Modified, 500, 0);
    f.patch = std::iter::once("diff --git a/src/big.rs b/src/big.rs".to_string())
        .chain((0..500).map(|i| format!("+line {i}")))
        .collect::<Vec<_>>()
        .join("\n");
    let out = render(
        &[f],
        Limits {
            per_file_lines: 10,
            total_chars: 100_000,
        },
    );
    assert!(out.contains("+line 8"));
    assert!(!out.contains("+line 9\n"));
    assert!(out.contains("491 more lines truncated"));
}

#[test]
fn render_omits_patches_over_total_budget() {
    let mut a = FileChange::new("a.rs", Status::Modified, 1, 0);
    a.patch = format!("diff --git a/a.rs b/a.rs\n+{}", "x".repeat(300));
    let mut b = FileChange::new("b.rs", Status::Modified, 1, 0);
    b.patch = format!("diff --git a/b.rs b/b.rs\n+{}", "y".repeat(300));
    let out = render(
        &[a, b],
        Limits {
            per_file_lines: 100,
            total_chars: 400,
        },
    );
    assert!(out.contains(&"x".repeat(300)));
    assert!(!out.contains("yyyy"));
    assert!(out.contains("M b.rs (+1 -0) [diff omitted: size limit]"));
}

#[test]
fn render_caps_file_list() {
    let files: Vec<_> = (0..120)
        .map(|i| FileChange::new(&format!("f{i}.rs"), Status::Added, 1, 0))
        .collect();
    let out = render(&files, Limits::default());
    assert!(out.contains("f49.rs"));
    assert!(!out.contains("f50.rs"));
    assert!(out.contains("... and 70 more files"));
}
