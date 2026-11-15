//! Source-built coverage for Java's floating-point Math.max/min overloads.
//!
//! Expected bit patterns were recorded from the real Java 25 JDK with
//! javac --release 21: NaN propagates and signed zero follows Java's tie rule.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;
public final class MathFloatEntrypoint {
  public static double maxDouble(double left, double right) { return Math.max(left, right); }
  public static double minDouble(double left, double right) { return Math.min(left, right); }
  public static float maxFloat(float left, float right) { return Math.max(left, right); }
  public static float minFloat(float left, float right) { return Math.min(left, right); }
}
"#;

fn build_fixture() -> scratch::Scratch {
    let root = scratch::Scratch::new("math-float");
    let source_path = root.join("src/demo/MathFloatEntrypoint.java");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    fs::write(&source_path, SOURCE).unwrap();
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "21", "-d"])
        .arg(root.join("classes"))
        .arg(&source_path)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");

    let jar_path = root.join("fixture-math-float.jar");
    let file = fs::File::create(&jar_path).unwrap();
    let mut jar = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    jar.start_file("fabric.mod.json", options).unwrap();
    jar.write_all(
        br#"{"id":"fixture-math-float","version":"1.0","entrypoints":{"main":"demo.MathFloatEntrypoint"}}"#,
    )
    .unwrap();
    jar.start_file("demo/MathFloatEntrypoint.class", options)
        .unwrap();
    jar.write_all(&fs::read(root.join("classes/demo/MathFloatEntrypoint.class")).unwrap())
        .unwrap();
    jar.finish().unwrap();
    root.into_path(jar_path)
}

fn call(jar: &NativeJar, name: &str, descriptor: &str, args: &[Value]) -> Value {
    jar.invoke_method_with_args(
        "demo/MathFloatEntrypoint",
        name,
        descriptor,
        None,
        args,
        &mut StdHost::new(),
    )
    .unwrap()
}

#[test]
fn floating_math_overloads_match_java25() {
    let fixture = build_fixture();
    let jar = NativeJar::from_path(&fixture).unwrap();

    let dmax = call(
        &jar,
        "maxDouble",
        "(DD)D",
        &[Value::Float(2.5), Value::Float(-1.0)],
    );
    assert!(matches!(dmax, Value::Float(value) if value == 2.5));
    let dnan = call(
        &jar,
        "maxDouble",
        "(DD)D",
        &[Value::Float(f64::NAN), Value::Float(1.0)],
    );
    assert!(matches!(dnan, Value::Float(value) if value.is_nan()));
    let dzmax = call(
        &jar,
        "maxDouble",
        "(DD)D",
        &[Value::Float(-0.0), Value::Float(0.0)],
    );
    assert!(matches!(dzmax, Value::Float(value) if value.to_bits() == 0));
    let dzmin = call(
        &jar,
        "minDouble",
        "(DD)D",
        &[Value::Float(0.0), Value::Float(-0.0)],
    );
    assert!(matches!(dzmin, Value::Float(value) if value.to_bits() == 0x8000_0000_0000_0000));

    let fmax = call(
        &jar,
        "maxFloat",
        "(FF)F",
        &[Value::Float(2.5), Value::Float(-1.0)],
    );
    assert!(matches!(fmax, Value::Float(value) if (value as f32) == 2.5));
    let fnan = call(
        &jar,
        "minFloat",
        "(FF)F",
        &[Value::Float(f64::NAN), Value::Float(1.0)],
    );
    assert!(matches!(fnan, Value::Float(value) if (value as f32).to_bits() == 0x7fc0_0000));
    let fzmax = call(
        &jar,
        "maxFloat",
        "(FF)F",
        &[Value::Float(-0.0), Value::Float(0.0)],
    );
    assert!(matches!(fzmax, Value::Float(value) if (value as f32).to_bits() == 0));
    let fzmin = call(
        &jar,
        "minFloat",
        "(FF)F",
        &[Value::Float(0.0), Value::Float(-0.0)],
    );
    assert!(matches!(fzmin, Value::Float(value) if (value as f32).to_bits() == 0x8000_0000));
}
