//! Java 25 oracle for the small NeoForge loader/API slice.
//!
//! The native probe uses a tiny synthetic jar; the expected rows in the
//! committed oracle file come from a real Java 25 run against the real
//! NeoForge 26.3.0.101/FML jars, with the command recorded there.
use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
use zip::write::SimpleFileOptions;

#[path = "support/scratch.rs"]
mod scratch;

const STUBS: &[(&str, &str)] = &[
    (
        "net/neoforged/fml/common/Mod.java",
        "package net.neoforged.fml.common; import java.lang.annotation.Retention; import java.lang.annotation.RetentionPolicy; @Retention(RetentionPolicy.RUNTIME) public @interface Mod { String value(); }",
    ),
    (
        "net/neoforged/fml/ModContainer.java",
        "package net.neoforged.fml; import net.neoforged.bus.api.IEventBus; import net.neoforged.neoforgespi.language.IModInfo; public class ModContainer { public String getModId(){return null;} public String getNamespace(){return null;} public IModInfo getModInfo(){return null;} public IEventBus getEventBus(){return null;} public void registerExtensionPoint(Class<?> type, IExtensionPoint point) {} }",
    ),
    (
        "net/neoforged/bus/api/IEventBus.java",
        "package net.neoforged.bus.api; public interface IEventBus {}",
    ),
    (
        "net/neoforged/neoforgespi/language/IModInfo.java",
        "package net.neoforged.neoforgespi.language; import org.apache.maven.artifact.versioning.ArtifactVersion; public interface IModInfo { String getModId(); ArtifactVersion getVersion(); }",
    ),
    (
        "net/neoforged/fml/IExtensionPoint.java",
        "package net.neoforged.fml; public interface IExtensionPoint {}",
    ),
    (
        "net/neoforged/neoforge/network/registration/HandlerThread.java",
        "package net.neoforged.neoforge.network.registration; public enum HandlerThread { MAIN, NETWORK }",
    ),
    (
        "org/apache/maven/artifact/versioning/ArtifactVersion.java",
        "package org.apache.maven.artifact.versioning; public interface ArtifactVersion { String toString(); }",
    ),
    (
        "net/neoforged/fml/ModLoadingContext.java",
        "package net.neoforged.fml; public class ModLoadingContext { public static ModLoadingContext get(){return null;} public ModContainer getActiveContainer(){return null;} public String getActiveNamespace(){return null;} }",
    ),
    (
        "net/neoforged/fml/loading/FMLLoader.java",
        "package net.neoforged.fml.loading; public class FMLLoader { public static boolean isProduction(){return false;} public static java.nio.file.Path getGameDir(){return null;} }",
    ),
    (
        "net/neoforged/neoforge/data/loading/DatagenModLoader.java",
        "package net.neoforged.neoforge.data.loading; public class DatagenModLoader { public static boolean isRunningDataGen(){return true;} }",
    ),
    (
        "net/neoforged/neoforge/network/event/RegisterPayloadHandlersEvent.java",
        "package net.neoforged.neoforge.network.event; import net.neoforged.neoforge.network.registration.PayloadRegistrar; public class RegisterPayloadHandlersEvent { public PayloadRegistrar registrar(String v){return null;} }",
    ),
    (
        "net/neoforged/neoforge/network/registration/PayloadRegistrar.java",
        "package net.neoforged.neoforge.network.registration; public class PayloadRegistrar { public PayloadRegistrar versioned(String v){return null;} public PayloadRegistrar optional(){return null;} public PayloadRegistrar executesOn(HandlerThread t){return null;} public PayloadRegistrar playToServer(Object type, Object codec, Object handler){return this;} }",
    ),
];

const ORACLE: &str = include_str!("fixtures/neoforge_api_slice/java25.txt");

fn oracle_rows() -> String {
    ORACLE
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

const PROBE: &str = r#"package demo;

import net.neoforged.fml.ModContainer;
import net.neoforged.fml.ModLoadingContext;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.data.loading.DatagenModLoader;
import net.neoforged.fml.loading.FMLLoader;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;
import net.neoforged.neoforge.network.registration.PayloadRegistrar;

@Mod("demo")
public final class Probe {
    private static String active;

    public Probe() {
        ModLoadingContext context = ModLoadingContext.get();
        ModContainer container = context.getActiveContainer();
        active = answeredRows(context, container);
    }

    public static String active() { return active; }

    public static String outside() {
        ModLoadingContext context = ModLoadingContext.get();
        return context.getActiveContainer().getModId() + "|"
            + context.getActiveContainer().getNamespace() + "|"
            + (context.getActiveContainer().getEventBus() == null) + "|"
            + context.getActiveContainer().getModInfo().getModId() + "|"
            + context.getActiveNamespace();
    }

    public static String contextIdentity() {
        return String.valueOf(ModLoadingContext.get() == ModLoadingContext.get());
    }

    private static String answeredRows(ModLoadingContext context, ModContainer container) {
        RegisterPayloadHandlersEvent event = new RegisterPayloadHandlersEvent();
        PayloadRegistrar registrar = event.registrar("1");
        PayloadRegistrar versioned = registrar.versioned("2");
        PayloadRegistrar optional = registrar.optional();
        PayloadRegistrar threaded = registrar.executesOn(null);
        boolean emptyVersionNoThrow;
        try {
            registrar.versioned("");
            emptyVersionNoThrow = true;
        } catch (Throwable error) {
            emptyVersionNoThrow = false;
        }
        String badCast;
        try {
            String ignored = (String) (Object) registrar;
            badCast = "no-error";
        } catch (ClassCastException error) {
            badCast = error.getMessage();
        }
        return "contextIdentity=" + (context == ModLoadingContext.get()) + "\n"
            + "activeModId=" + container.getModId() + "\n"
            + "activeNamespace=" + container.getNamespace() + "\n"
            + "activeEventBusIdentity=" + (container.getEventBus() == container.getEventBus()) + "\n"
            + "activeInfoModId=" + container.getModInfo().getModId() + "\n"
            + "activeInfoVersion=" + container.getModInfo().getVersion() + "\n"
            + "registrarClass=" + registrar.getClass().getName() + "\n"
            + "registrarVersionedDistinct=" + (registrar != versioned) + "\n"
            + "registrarOptionalDistinct=" + (registrar != optional) + "\n"
            + "registrarExecutesOnDistinct=" + (registrar != threaded) + "\n"
            + "registrarEmptyVersionNoThrow=" + emptyVersionNoThrow + "\n"
            + "registrarCast=" + (((Object) registrar) instanceof PayloadRegistrar) + "\n"
            + "registrarBadCast=" + badCast;
    }

    public static String answered() { return active; }

    public static String counted() {
        ModContainer container = ModLoadingContext.get().getActiveContainer();
        container.registerExtensionPoint(Extension.class, new Extension());
        PayloadRegistrar registrar = new RegisterPayloadHandlersEvent().registrar("1");
        PayloadRegistrar versioned = registrar.versioned("2");
        PayloadRegistrar optional = registrar.optional();
        PayloadRegistrar threaded = registrar.executesOn(null);
        versioned.playToServer(null, null, null);
        optional.playToServer(null, null, null);
        threaded.playToServer(null, null, null);
        return "done";
    }

    public static final class Extension implements net.neoforged.fml.IExtensionPoint {}

    public static ModLoadingContext context() { return ModLoadingContext.get(); }

    public static String dataGen() {
        return String.valueOf(DatagenModLoader.isRunningDataGen());
    }

    public static String gameDir() { return FMLLoader.getGameDir().toString(); }
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

fn text(jar: &NativeJar, host: &mut StdHost, method: &str) -> String {
    match jar
        .invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            host,
        )
        .unwrap()
    {
        Value::Ref(Some(value)) => value,
        other => panic!("{method}: expected text, got {other:?}"),
    }
}

#[test]
fn neoforge_loader_oracle_rows_and_counted_unknowns_are_exact() {
    let root = scratch::Scratch::new("neoforge-api");
    let src = root.join("src");
    let classes = root.join("classes");
    for (relative, body) in STUBS {
        let path = src.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    let probe = src.join("demo/Probe.java");
    fs::create_dir_all(probe.parent().unwrap()).unwrap();
    fs::write(&probe, PROBE).unwrap();
    let sources = files_under(&src);
    fs::create_dir_all(&classes).unwrap();
    let status = Command::new(javac())
        .args(["--release", "21", "-d"])
        .arg(&classes)
        .args(&sources)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");

    let jar_path = root.join("neoforge-api.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    zip.start_file("META-INF/neoforge.mods.toml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"[[mods]]\nmodId = \"demo\"\nversion = \"1.0\"\n")
        .unwrap();
    for class in files_under(&classes.join("demo")) {
        let name = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();

    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = jar.seed_host(StdHost::new().with_mod("demo", "1.0"));
    jar.execute("demo.Probe", None, "neoforge", &mut host)
        .unwrap();
    assert_eq!(text(&jar, &mut host, "answered"), oracle_rows());
    let before_unknown = host.unknown_host_calls().to_vec();
    let before_payloads = host.payload_registrations().len();
    assert_eq!(text(&jar, &mut host, "counted"), "done",);
    let register_call = "net/neoforged/fml/ModContainer#registerExtensionPoint(Ljava/lang/Class;Lnet/neoforged/fml/IExtensionPoint;)V";
    let before_register = before_unknown
        .iter()
        .find(|(call, _)| call == register_call)
        .map_or(0, |(_, count)| *count);
    let after_register = host
        .unknown_host_calls()
        .iter()
        .find(|(call, _)| call == register_call)
        .map_or(0, |(_, count)| *count);
    assert!(
        after_register > before_register,
        "registerExtensionPoint was not counted"
    );
    for refinement in [
        "net/neoforged/neoforge/network/registration/PayloadRegistrar#versioned(Ljava/lang/String;)Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;",
        "net/neoforged/neoforge/network/registration/PayloadRegistrar#optional()Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;",
        "net/neoforged/neoforge/network/registration/PayloadRegistrar#executesOn(Lnet/neoforged/neoforge/network/registration/HandlerThread;)Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;",
    ] {
        let before = before_unknown
            .iter()
            .find(|(call, _)| call == refinement)
            .map_or(0, |(_, count)| *count);
        let after = host
            .unknown_host_calls()
            .iter()
            .find(|(call, _)| call == refinement)
            .map_or(0, |(_, count)| *count);
        assert_eq!(
            after, before,
            "exact refinement unexpectedly counted: {refinement}"
        );
    }
    assert_eq!(host.payload_registrations().len(), before_payloads + 3);
    let registrations = &host.payload_registrations()[before_payloads..];
    assert_eq!(
        registrations[0].mod_id,
        Value::Ref(Some("minecraft".into()))
    );
    assert_eq!(registrations[0].version, Value::Ref(Some("2".into())));
    assert!(!registrations[0].optional);
    assert_eq!(registrations[0].thread, Value::Ref(Some("MAIN".into())));
    assert_eq!(
        registrations[1].mod_id,
        Value::Ref(Some("minecraft".into()))
    );
    assert_eq!(registrations[1].version, Value::Ref(Some("1".into())));
    assert!(registrations[1].optional);
    assert_eq!(registrations[1].thread, Value::Ref(Some("MAIN".into())));
    assert_eq!(
        registrations[2].mod_id,
        Value::Ref(Some("minecraft".into()))
    );
    assert_eq!(registrations[2].version, Value::Ref(Some("1".into())));
    assert!(!registrations[2].optional);
    assert_eq!(registrations[2].thread, Value::Ref(None));
    let mut datagen = jar.seed_host(StdHost::new().with_mod("demo", "1.0"));
    let before = datagen.unknown_host_call_count();
    let _ = text(&jar, &mut datagen, "dataGen");
    assert!(datagen.unknown_host_call_count() > before);
    assert!(
        datagen
            .unknown_host_calls()
            .iter()
            .any(|(call, count)| call.contains("DatagenModLoader#isRunningDataGen") && *count >= 1)
    );

    // FML's context is a ThreadLocal: equal reads on one native thread share
    // an object, while a context obtained on another thread is different.
    let jar = Arc::new(jar);
    let first = jar
        .invoke_method(
            "demo/Probe",
            "context",
            "()Lnet/neoforged/fml/ModLoadingContext;",
            None,
            &mut host,
        )
        .unwrap();
    let other_jar = Arc::clone(&jar);
    let other = std::thread::spawn(move || {
        let mut other_host = other_jar.seed_host(StdHost::new().with_mod("demo", "1.0"));
        other_jar
            .invoke_method(
                "demo/Probe",
                "context",
                "()Lnet/neoforged/fml/ModLoadingContext;",
                None,
                &mut other_host,
            )
            .unwrap()
    })
    .join()
    .unwrap();
    assert_ne!(first, other);
}
