//! Java 25 fixture for the completed `CompletableFuture` slice.

use horizon_native_java::{NativeError, NativeJar, StdHost, Value};

const ORACLE: &str = include_str!("fixtures/completed_future/java25.txt");
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zip::write::SimpleFileOptions;

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

const SOURCE: &str = r#"
package demo;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionStage;
import java.util.concurrent.CompletionException;
import java.util.concurrent.Future;

public final class CompletedFutureProbe {
  public static String chain() throws Exception {
    StringBuilder seen = new StringBuilder();
    CompletableFuture<String> base = CompletableFuture.completedFuture("a");
    CompletableFuture<String> mapped = base.thenApply(value -> value + "b");
    CompletableFuture<String> composed = mapped.thenCompose(
        value -> CompletableFuture.completedFuture(value + "c"));
    CompletableFuture<Void> accepted = composed.thenAccept(seen::append);
    return base.isDone() + "|" + mapped.get() + "|" + composed.join()
        + "|" + accepted.isDone() + "|" + seen;
  }

  public static String exceptional() {
    CompletableFuture<String> bad = CompletableFuture.failedFuture(new IllegalStateException("bad"));
    StringBuilder seen = new StringBuilder();
    String join;
    try { bad.join(); join = "none"; }
    catch (CompletionException e) { join = e.getClass().getSimpleName() + ":" + e.getCause().getClass().getSimpleName() + ":" + e.getCause().getMessage(); }
    String get;
    try { bad.get(); get = "none"; }
    catch (Exception e) { get = e.getClass().getSimpleName() + ":" + e.getCause().getClass().getSimpleName() + ":" + e.getCause().getMessage(); }
    String now;
    try { bad.getNow("fallback"); now = "none"; }
    catch (CompletionException e) { now = e.getClass().getSimpleName(); }
    CompletableFuture<String> recovered = bad.exceptionally(t -> "recovered:" + t.getMessage());
    CompletableFuture<String> handled = bad.handle((value, t) -> t.getClass().getSimpleName());
    CompletableFuture<String> observed = bad.whenComplete((value, t) -> seen.append(t.getMessage()));
    String suppressed;
    try {
      bad.whenComplete((value, t) -> { throw new IllegalArgumentException("wc"); }).join();
      suppressed = "none";
    } catch (CompletionException e) {
      suppressed = e.getCause().getSuppressed()[0].getMessage();
    }
    String callback;
    try {
      CompletableFuture.completedFuture("x").thenApply(value -> { throw new RuntimeException("boom"); }).join();
      callback = "none";
    } catch (CompletionException e) { callback = e.getCause().getMessage(); }
    return join + "|" + get + "|" + now + "|" + bad.isCompletedExceptionally()
        + "|" + recovered.join() + "|" + handled.join() + "|" + observed.isCompletedExceptionally()
        + "|" + seen + "|" + suppressed + "|" + callback;
  }

  public static String alreadyWrapped() {
    CompletableFuture<String> bad = CompletableFuture.failedFuture(
        new CompletionException(new IllegalStateException("wrapped")));
    try { bad.join(); return "none"; }
    catch (CompletionException e) {
      return e.getClass().getSimpleName() + "|" + e.getCause().getClass().getSimpleName()
          + "|" + e.getCause().getMessage();
    }
  }

  public static String toStringCall() {
    return CompletableFuture.completedFuture("x").toString();
  }

  public static String completeAlready() {
    CompletableFuture<String> future = CompletableFuture.completedFuture("x");
    boolean first = future.complete("y");
    boolean second = future.completeExceptionally(new IllegalStateException("late"));
    return first + "|" + second + "|" + future.getNow("z") + "|" + future.isCompletedExceptionally();
  }

  public static String interfaces() throws Exception {
    CompletionStage<String> stage = CompletableFuture.completedFuture("x");
    Future<String> future = CompletableFuture.completedFuture("y");
    return stage.thenApply(value -> value + "1").toCompletableFuture().join()
        + "|" + future.get() + "|" + future.isDone();
  }

  public static String allOf() {
    return String.valueOf(CompletableFuture.allOf(
        CompletableFuture.completedFuture("a"), CompletableFuture.completedFuture("b")).isDone());
  }

  public static String anyOf() {
    return String.valueOf(CompletableFuture.anyOf(
        CompletableFuture.completedFuture("a"), CompletableFuture.completedFuture("b")).join());
  }

  public static String obtrudeValue() {
    CompletableFuture<String> future = CompletableFuture.failedFuture(new IllegalStateException("old"));
    future.obtrudeValue("z");
    return future.join() + "|" + future.isCompletedExceptionally();
  }

  public static String obtrudeException() {
    CompletableFuture<String> future = CompletableFuture.completedFuture("x");
    future.obtrudeException(new IllegalStateException("late"));
    try { future.join(); return "none"; }
    catch (CompletionException e) { return e.getCause().getMessage() + "|" + future.isCompletedExceptionally(); }
  }

  public static String async() {
    return CompletableFuture.supplyAsync(() -> "never").join();
  }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn fixture() -> NativeJar {
    let serial = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-completed-future-{}-{serial}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/demo")).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    let source = root.join("src/demo/CompletedFutureProbe.java");
    fs::write(&source, SOURCE).unwrap();
    let status = Command::new(javac())
        .args(["--release", "21", "-d"])
        .arg(root.join("classes"))
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    let jar_path = root.join("completed-future.jar");
    let mut jar = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    jar.start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    jar.write_all(
        br#"{"id":"completed-future-fixture","version":"1.0","entrypoints":{"main":"demo.CompletedFutureProbe"}}"#,
    )
    .unwrap();
    jar.start_file(
        "demo/CompletedFutureProbe.class",
        SimpleFileOptions::default(),
    )
    .unwrap();
    jar.write_all(&fs::read(root.join("classes/demo/CompletedFutureProbe.class")).unwrap())
        .unwrap();
    jar.finish().unwrap();
    let result = NativeJar::from_path(&jar_path).unwrap();
    let _ = fs::remove_dir_all(root);
    result
}

fn call(jar: &NativeJar, method: &str, host: &mut StdHost) -> Result<Value, NativeError> {
    jar.invoke_method_with_args(
        "demo/CompletedFutureProbe",
        method,
        "()Ljava/lang/String;",
        None,
        &[],
        host,
    )
}

#[test]
fn completed_future_chain_matches_java25_oracle() {
    let jar = fixture();
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    let expected: Vec<_> = ORACLE.lines().collect();
    assert_eq!(
        call(&jar, "chain", &mut host).unwrap(),
        Value::Ref(Some(expected[0].into()))
    );
    assert_eq!(
        call(&jar, "interfaces", &mut host).unwrap(),
        Value::Ref(Some(expected[1].into()))
    );
}

#[test]
fn exceptional_future_semantics_match_java25_oracle() {
    let jar = fixture();
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    let expected: Vec<_> = ORACLE.lines().collect();
    assert_eq!(
        call(&jar, "exceptional", &mut host).unwrap(),
        Value::Ref(Some(expected[2].into()))
    );
    assert_eq!(
        call(&jar, "completeAlready", &mut host).unwrap(),
        Value::Ref(Some(expected[3].into()))
    );
}

#[test]
fn future_to_string_remains_counted() {
    let jar = fixture();
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    let error = call(&jar, "toStringCall", &mut host).unwrap_err();
    assert!(matches!(error, NativeError::UnknownHostCall(_)));
    assert!(host.unknown_host_calls().iter().any(|(call, _)| {
        call == "java/util/concurrent/CompletableFuture#toString()Ljava/lang/String;"
    }));
}

#[test]
fn unmodeled_combinators_and_obtrusion_remain_counted() {
    let jar = fixture();
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    for method in ["allOf", "anyOf", "obtrudeValue", "obtrudeException"] {
        let error = call(&jar, method, &mut host).unwrap_err();
        assert!(matches!(error, NativeError::UnknownHostCall(_)));
    }
}

#[test]
fn async_factory_remains_counted() {
    let jar = fixture();
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    let error = call(&jar, "async", &mut host).unwrap_err();
    assert!(matches!(error, NativeError::UnknownHostCall(_)));
    assert!(host.unknown_host_calls().iter().any(|(call, _)| {
        call == "java/util/concurrent/CompletableFuture#supplyAsync(Ljava/util/function/Supplier;)Ljava/util/concurrent/CompletableFuture;"
    }));
}
