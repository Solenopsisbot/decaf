//! Java 25 fixture for the bounded BigDecimal slice.
#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const ORACLE: &str = include_str!("fixtures/bigdecimal_slice/oracle/java25.txt");

const PROBE: &str = r#"package demo;
import java.math.BigDecimal;
import java.math.RoundingMode;

public final class Probe {
    public static String values() {
        BigDecimal fromLong = BigDecimal.valueOf(123400L, 4);
        BigDecimal fromDouble = BigDecimal.valueOf(1.5d);
        BigDecimal fromString = new BigDecimal("12.3400");
        BigDecimal stripped = fromString.stripTrailingZeros();
        BigDecimal rounded = new BigDecimal("1.235").setScale(2, RoundingMode.HALF_UP);
        BigDecimal down = new BigDecimal("10.00").setScale(1);
        BigDecimal zero = new BigDecimal("0.00");
        BigDecimal zeroExp = new BigDecimal("0E+3");
        return fromLong.toPlainString() + "|" + fromLong.toString() + "|"
            + fromDouble.toPlainString() + "|" + fromString.scale() + "|"
            + fromString.precision() + "|" + stripped.toPlainString() + "|"
            + stripped.toString() + "|" + rounded.toPlainString() + "|"
            + down.toPlainString() + "|" + fromString.equals(new BigDecimal("12.3400")) + "|"
            + fromString.equals(new BigDecimal("12.34")) + "|" + fromString.compareTo(new BigDecimal("12.34")) + "|"
            + fromString.hashCode() + "|" + new BigDecimal("12.34").hashCode() + "|"
            + zero.toPlainString() + "|" + zero.toString() + "|" + zero.scale() + "|"
            + zero.hashCode() + "|" + zeroExp.toString() + "|" + zeroExp.toPlainString();
    }

    public static String reviewEdges() {
        StringBuilder out = new StringBuilder();
        for (double value : new double[] { Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY }) {
            try { BigDecimal.valueOf(value); out.append("no"); }
            catch (RuntimeException e) { out.append(e.getClass().getSimpleName()).append(':').append(e.getMessage()).append('|'); }
        }
        out.append(BigDecimal.valueOf(0) == BigDecimal.valueOf(0)).append('|')
            .append(BigDecimal.valueOf(1) == BigDecimal.valueOf(1)).append('|')
            .append(BigDecimal.valueOf(10) == BigDecimal.valueOf(10)).append('|')
            .append(BigDecimal.valueOf(0, 0) == BigDecimal.ZERO).append('|')
            .append(BigDecimal.ZERO.stripTrailingZeros() == BigDecimal.ZERO).append('|');
        try { new BigDecimal("1.2").setScale(0, null); out.append("no"); }
        catch (RuntimeException e) { out.append(e.getClass().getSimpleName()).append(':').append(e.getMessage()).append('|'); }
        out.append(BigDecimal.valueOf(Long.MIN_VALUE).hashCode());
        return out.toString();
    }

    public static String zeroScaleRows() {
        StringBuilder out = new StringBuilder();
        for (int scale : new int[] {0, 1, 2, 6, 7, 8, -1, -7}) {
            out.append(scale).append('=').append(BigDecimal.ZERO.setScale(scale).toString()).append('|');
        }
        BigDecimal x = BigDecimal.valueOf(-5736997553998261837L, 87);
        for (RoundingMode mode : new RoundingMode[] {
            RoundingMode.DOWN, RoundingMode.CEILING, RoundingMode.FLOOR,
            RoundingMode.HALF_UP, RoundingMode.HALF_DOWN, RoundingMode.HALF_EVEN
        }) {
            out.append(mode).append('=').append(x.setScale(7, mode).toString()).append('|');
        }
        return out.toString();
    }

    public static String cacheRows() {
        StringBuilder out = new StringBuilder();
        for (long n = 0; n <= 10; n++) {
            BigDecimal value = BigDecimal.valueOf(n);
            out.append(n).append(':')
                .append(value == BigDecimal.valueOf(n)).append(':')
                .append(value == BigDecimal.valueOf(n, 0)).append(':')
                .append(BigDecimal.valueOf(n, 0) == BigDecimal.valueOf(n, 0)).append('|');
        }
        return out.toString();
    }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

struct Fixture {
    root: PathBuf,
    jar: NativeJar,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn build() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = scratch::unique_path(&format!(
            "horizon-native-java-bigdecimal-{}-{unique}",
            std::process::id()
        ));
        let classes = root.join("classes");
        let source = root.join("src/demo/Probe.java");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, PROBE).unwrap();
        let mut javac = Command::new(javac());
        javac
            .args(["--release", "25", "-d"])
            .arg(&classes)
            .arg(&source);
        assert!(
            javac.status().unwrap().success(),
            "javac failed for BigDecimal fixture"
        );
        let jar_path = root.join("bigdecimal.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"bigdecimal-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for class in files_under(&classes) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();
        Self {
            root,
            jar: NativeJar::from_path(&jar_path).unwrap(),
        }
    }
}

#[test]
fn java25_bigdecimal_slice_matches_the_jdk() {
    let fixture = Fixture::build();
    let mut host = StdHost::new();
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "values",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let Value::Ref(Some(text)) = answer else {
        panic!("not text: {answer:?}")
    };
    assert_eq!(
        text,
        ORACLE
            .lines()
            .find(|line| line.starts_with("values="))
            .unwrap()
            .split_once("=")
            .unwrap()
            .1
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn java25_review_edges_match_the_jdk() {
    let fixture = Fixture::build();
    let mut host = StdHost::new();
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "reviewEdges",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let Value::Ref(Some(text)) = answer else {
        panic!("not text: {answer:?}")
    };
    assert_eq!(
        text,
        ORACLE
            .lines()
            .find(|line| line.starts_with("reviewEdges="))
            .unwrap()
            .split_once("=")
            .unwrap()
            .1
    );

    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn java25_zero_scale_rows_match_layout_chars() {
    let fixture = Fixture::build();
    let mut host = StdHost::new();
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "zeroScaleRows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let Value::Ref(Some(text)) = answer else {
        panic!("not text: {answer:?}")
    };
    assert_eq!(
        text,
        ORACLE
            .lines()
            .find(|line| line.starts_with("zeroScaleRows="))
            .unwrap()
            .split_once("=")
            .unwrap()
            .1
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn java25_small_value_cache_identities_match_the_jdk() {
    let fixture = Fixture::build();
    let mut host = StdHost::new();
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "cacheRows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let Value::Ref(Some(text)) = answer else {
        panic!("not text: {answer:?}")
    };
    assert_eq!(
        text,
        ORACLE
            .lines()
            .find(|line| line.starts_with("cacheRows="))
            .unwrap()
            .split_once("=")
            .unwrap()
            .1
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
