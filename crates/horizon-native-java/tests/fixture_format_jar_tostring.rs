#[path = "support/scratch.rs"]
mod scratch;
use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;
const SOURCE: &str = include_str!("fixtures/format_jar_tostring/demo/Probe.java");
fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|h| PathBuf::from(h).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}
fn jar() -> NativeJar {
    let r = scratch::Scratch::new("format-jar-tostring");
    let src = r.join("Probe.java");
    fs::write(&src, SOURCE).unwrap();
    fs::create_dir_all(r.join("classes")).unwrap();
    assert!(
        Command::new(javac())
            .args(["-d"])
            .arg(r.join("classes"))
            .arg(&src)
            .status()
            .unwrap()
            .success()
    );
    let p = r.join("x.jar");
    let mut z = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    for f in [
        "Probe.class",
        "Probe$Named.class",
        "Probe$Nully.class",
        "Probe$Boom.class",
        "Probe$Fmt.class",
    ] {
        z.start_file(format!("demo/{f}"), SimpleFileOptions::default())
            .unwrap();
        z.write_all(&fs::read(r.join("classes/demo").join(f)).unwrap())
            .unwrap();
    }
    z.finish().unwrap();
    NativeJar::from_path(&p).unwrap()
}
fn call(j: &NativeJar, h: &mut StdHost, n: &str) -> Value {
    j.invoke_method_with_args("demo.Probe", n, "()Ljava/lang/String;", None, &[], h)
        .unwrap()
}
#[test]
fn formatter_jar_tostring_capture() {
    let j = jar();
    let mut h = StdHost::new();
    let v = call(&j, &mut h, "rows");
    assert_eq!(
        v,
        Value::Ref(Some(
            include_str!("fixtures/format_jar_tostring/java25.txt").to_owned()
        ))
    );
    assert_eq!(h.unknown_host_calls(), vec![]);
}
/// Java 25's answers for the counted cases, one line each in `CASES` order.
const JAVA25_COUNTED: &str = include_str!("fixtures/format_jar_tostring/java25_counted.txt");

const FORMATTED: &str = "java/lang/String#formatted([Ljava/lang/Object;)Ljava/lang/String;";
const FORMAT: &str =
    "java/lang/String#format(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;";

/// Cases Decaf counts, each in a fresh host: a jar `toString()` that throws (Java throws it
/// from `format`; the host's `jar_to_string` refuses a throwing `toString`, so the format is
/// counted), a jar `Formattable` (its `formatTo` isn't run), and a non-`%s` conversion of a jar
/// object (`%d` throws `IllegalFormatConversionException` in Java; counted here). A counted
/// `format` returns null without throwing, so the probe's own result isn't Java's: only the
/// count is asserted, and `java25_counted.txt` records what Java answers.
#[test]
fn formatter_counted_cases() {
    let cases = [("fmt", FORMATTED), ("mixed", FORMAT)];
    assert_eq!(cases.len(), JAVA25_COUNTED.lines().count());
    for (name, signature) in cases {
        let jar = jar();
        let mut host = StdHost::new();
        call(&jar, &mut host, name);
        assert_eq!(
            host.unknown_host_calls(),
            vec![(signature.to_owned(), 1)],
            "{name}"
        );
    }
}
