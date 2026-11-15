//! `String.format` and `PrintStream.printf` from a javac-built class, so the
//! boxing calls, descriptors and the `pop` after `printf` are javac's own.
//!
//! The formatter answers what it reproduces exactly and leaves the rest to the
//! host as an unknown, counted call. Expected strings are what the same class
//! printed on OpenJDK 27 and OpenJDK 17.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;

import java.util.Formattable;
import java.util.Formatter;
import java.util.Locale;

public final class FormatEntrypoint {
  private static final class F implements Formattable {
    @Override public void formatTo(Formatter formatter, int flags, int width, int precision) {
      formatter.format("<F:%d:%d:%d>", flags, width, precision);
    }
  }

  public static String boxes() {
    return String.format("%b %b %B|%x %c %s", Boolean.FALSE, Boolean.TRUE, null, (short) -2, 'x', 7L);
  }

  public static String rounding() {
    return String.format(Locale.ROOT, "%f|%.2e|%g|%(,.1f", 5e-7, 9.995, 1e-4, -1234.25);
  }

  public static String printed() {
    System.out.printf("%s=%,d%n", "ticks", 1234567);
    System.out.printf(Locale.US, "[%-7.2f]%n", -0.0);
    return "done";
  }

  public static String formattable() {
    return String.format("%s|%b|%s", new F(), true, null);
  }

  public static String unmodelled() {
    return String.format("%a", 1.0);
  }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

/// Compile the fixture class into a jar of its own; each test passes its own
/// tag so parallel tests never share a build root.
fn jar(tag: &str) -> NativeJar {
    let root = scratch::Scratch::new(&format!("format-{tag}"));
    let source = root.join("src/demo/FormatEntrypoint.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    fs::write(&source, SOURCE).unwrap();
    let status = Command::new(javac())
        .args(["--release", "17", "-d"])
        .arg(root.join("classes"))
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    let jar_path = root.join("format.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    for class in ["FormatEntrypoint.class", "FormatEntrypoint$F.class"] {
        zip.start_file(format!("demo/{class}"), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&fs::read(root.join("classes/demo").join(class)).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();
    NativeJar::from_path(&jar_path).unwrap()
}

fn call(jar: &NativeJar, host: &mut StdHost, name: &str) -> Value {
    jar.invoke_method_with_args(
        "demo.FormatEntrypoint",
        name,
        "()Ljava/lang/String;",
        None,
        &[],
        host,
    )
    .unwrap_or_else(|error| panic!("{name} failed: {error}"))
}

fn text(value: &str) -> Value {
    Value::Ref(Some(value.to_owned()))
}

#[test]
fn format_answers_boxed_arguments_and_rounding_like_the_jdk() {
    let jar = jar("answers");
    let mut host = StdHost::new();
    // `%b` of `Boolean.FALSE` is `false`; `%x` of a `short` is 16 bits wide.
    assert_eq!(
        call(&jar, &mut host, "boxes"),
        text("false true FALSE|fffe x 7")
    );
    // The shortest digits round half up: `5e-7` is `0.000001`.
    assert_eq!(
        call(&jar, &mut host, "rounding"),
        text("0.000001|1.00e+01|0.000100000|(1,234.3)")
    );
    assert_eq!(host.unknown_host_call_count(), 0);
}

#[test]
fn printf_writes_the_text_and_returns_its_stream() {
    let jar = jar("printf");
    let mut host = StdHost::new();
    // javac pops the `PrintStream` `printf` returns; the method only reaches
    // its `return` if the call pushed one.
    assert_eq!(call(&jar, &mut host, "printed"), text("done"));
    // The first printf uses the player's default FORMAT locale for %,d. The
    // bounded host cannot provide that locale, so it is counted and writes no text;
    // the explicit Locale.US call remains exact.
    assert_eq!(host.take_stdout(), "[-0.00  ]\n");
    assert_eq!(host.unknown_host_call_count(), 1);
}

#[test]
fn formattable_percent_s_is_counted_until_format_to_is_modelled() {
    let jar = jar("formattable");
    let mut host = StdHost::new();
    // Java 25 calls F.formatTo and produces <F:0:-1:-1>|true|null. The
    // bounded interpreter does not execute that callback yet, so it counts
    // the String.format call instead of falling back to Object.toString().
    assert_eq!(call(&jar, &mut host, "formattable"), Value::Ref(None));
    assert_eq!(host.unknown_host_call_count(), 1);
}

#[test]
fn an_unmodelled_conversion_is_an_unknown_host_call() {
    let jar = jar("unknown");
    let mut host = StdHost::new();
    // The JDK prints `0x1.0p0`; `%a` is not modelled, so the call is the
    // host's and counted rather than approximated.
    assert_eq!(call(&jar, &mut host, "unmodelled"), Value::Ref(None));
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "java/lang/String#format(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;"
                .to_owned(),
            1
        )]
    );
}
