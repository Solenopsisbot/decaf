//! Log4j 2.26.0 Level fixture compiled against a signature-only API stub.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use javac_fixture::Fixture;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const PROBE: &str = r#"
package demo;
import org.apache.logging.log4j.Level;
public final class Probe {
  public static String levels() {
    Level info=Level.INFO, error=Level.ERROR;
    String forged="l4lvl:INFO";
    String strict; try { Level.valueOf("missing"); strict="no"; } catch (IllegalArgumentException e) { strict="yes"; }
    Level custom=Level.forName("custom",350), repeat=Level.forName(" CUSTOM ",1);
    StringBuilder order=new StringBuilder(); for(Level level:Level.values()) order.append(level.name()).append(":").append(level.intLevel()).append(",");
    return (info==Level.valueOf(" info "))+"|"+info.name()+"|"+info.intLevel()+"|"+info.toString()
      +"|"+error.isMoreSpecificThan(info)+"|"+info.isLessSpecificThan(error)
      +"|"+error.isInRange(Level.ERROR,Level.DEBUG)+"|"+error.compareTo(info)
      +"|"+(Level.getLevel("missing")==null)+"|"+Level.toLevel("missing").toString()
      +"|"+Level.toLevel(null,Level.WARN).toString()+"|"+strict
      +"|"+custom.name()+"|"+custom.intLevel()+"|"+(custom==repeat)+"|"+(Level.valueOf("custom")==custom)
      +"|"+order+"|"+forged.getClass().getName()+"|"+forged.equals("l4lvl:INFO");
  }
}
"#;

const LEVEL_API_STUB: &str = r#"
package org.apache.logging.log4j;
public final class Level implements Comparable<Level> {
  public static final Level ALL = null, TRACE = null, DEBUG = null, INFO = null,
      WARN = null, ERROR = null, FATAL = null, OFF = null;
  public static Level forName(String name, int intLevel) { throw new UnsupportedOperationException(); }
  public static Level getLevel(String name) { throw new UnsupportedOperationException(); }
  public static Level valueOf(String name) { throw new UnsupportedOperationException(); }
  public static Level toLevel(String name) { throw new UnsupportedOperationException(); }
  public static Level toLevel(String name, Level defaultLevel) { throw new UnsupportedOperationException(); }
  public static Level[] values() { throw new UnsupportedOperationException(); }
  public String name() { throw new UnsupportedOperationException(); }
  public int intLevel() { throw new UnsupportedOperationException(); }
  public boolean isMoreSpecificThan(Level level) { throw new UnsupportedOperationException(); }
  public boolean isLessSpecificThan(Level level) { throw new UnsupportedOperationException(); }
  public boolean isInRange(Level minLevel, Level maxLevel) { throw new UnsupportedOperationException(); }
  @Override public int compareTo(Level level) { throw new UnsupportedOperationException(); }
  @Override public String toString() { throw new UnsupportedOperationException(); }
}
"#;

const EXPECTED: &str = "true|INFO|400|INFO|true|true|true|-1|true|DEBUG|WARN|yes|custom|350|true|true|TRACE:600,ALL:2147483647,custom:350,ERROR:200,INFO:400,FATAL:100,DEBUG:500,OFF:0,WARN:300,|java.lang.String|true";

fn javac_path() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn compile_api_stub() -> (PathBuf, String) {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "horizon-log4j-api-stub-{}-{unique}",
        std::process::id()
    ));
    let source = root.join("org/apache/logging/log4j/Level.java");
    let classes = root.join("classes");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, LEVEL_API_STUB).unwrap();
    let status = Command::new(javac_path())
        .args(["--release", "21", "-d"])
        .arg(&classes)
        .arg(&source)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "javac failed for the Log4j API signature stub"
    );
    let classpath = classes.to_string_lossy().into_owned();
    (root, classpath)
}

fn assert_levels(classpath: &str, name: &str) {
    let fixture = Fixture::build_with_classpath(name, &[("demo/Probe.java", PROBE)], classpath);
    assert_eq!(fixture.text("levels"), EXPECTED);
}

#[test]
fn log4j_level_surface_matches_java25_oracle() {
    // EXPECTED is recorded from Java 25.0.5 + the real Log4j API 2.26.0 run.
    let (stub_root, stub_classpath) = compile_api_stub();
    assert_levels(&stub_classpath, "log4j-levels-stub");
    let _ = fs::remove_dir_all(stub_root);

    if let Ok(api_jar) = std::env::var("LOG4J_API_JAR") {
        assert!(
            std::path::Path::new(&api_jar).is_file(),
            "missing real Log4j API jar: {api_jar}"
        );
        assert_levels(&api_jar, "log4j-levels-real");
    }
}
