#[path = "support/scratch.rs"]
mod scratch;
use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;
const SOURCE: &str = include_str!("fixtures/files_vfs/FilesProbe.java");
fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|h| PathBuf::from(h).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}
fn jar() -> NativeJar {
    let root = scratch::Scratch::new("files-vfs");
    let src = root.join("FilesProbe.java");
    fs::write(&src, SOURCE).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    assert!(
        Command::new(javac())
            .args(["--release", "17", "-d"])
            .arg(root.join("classes"))
            .arg(&src)
            .status()
            .unwrap()
            .success()
    );
    let jp = root.join("p.jar");
    let mut z = zip::ZipWriter::new(fs::File::create(&jp).unwrap());
    z.start_file("FilesProbe.class", SimpleFileOptions::default())
        .unwrap();
    z.write_all(&fs::read(root.join("classes/FilesProbe.class")).unwrap())
        .unwrap();
    z.finish().unwrap();
    NativeJar::from_path(&jp).unwrap()
}
#[test]
fn files_vfs_matches_java25_oracle() {
    let j = jar();
    let mut h = StdHost::new();
    let v = j
        .invoke_method_with_args(
            "FilesProbe",
            "rows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut h,
        )
        .unwrap();
    assert_eq!(
        v,
        Value::Ref(Some(
            include_str!("fixtures/files_vfs/java25.txt").to_owned()
        ))
    );
    // Counted: `SYNC`, `DSYNC` and `WRITE` over an existing file (Java overwrites in place).
    assert_eq!(h.unknown_host_calls(), &[("java/nio/file/Files#writeString(Ljava/nio/file/Path;Ljava/lang/CharSequence;[Ljava/nio/file/OpenOption;)Ljava/nio/file/Path;".into(), 3)]);
}
