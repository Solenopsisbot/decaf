//! Java 25 oracle for pure network codec construction.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::{NativeJar, StdHost, Value};

fn java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            java_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "java")
        {
            out.push(path);
        }
    }
}

fn jar() -> &'static NativeJar {
    static JAR: OnceLock<NativeJar> = OnceLock::new();
    JAR.get_or_init(|| {
        use std::io::Write;

        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let fixtures = [
            manifest.join("tests/fixtures/stream_codecs"),
            manifest.join("../crates/horizon-native-java/tests/fixtures/stream_codecs"),
        ]
        .into_iter()
        .find(|dir| dir.join("demo/StreamCodecs.java").is_file())
        .expect("the fixture is in crates/horizon-native-java/tests/fixtures/stream_codecs");
        let scratch = std::env::temp_dir().join(format!(
            "horizon-native-java-stream-codecs-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        let classes = scratch.join("classes");
        std::fs::create_dir_all(&classes).unwrap();

        let mut sources = Vec::new();
        java_files(&fixtures.join("stubs"), &mut sources);
        java_files(&fixtures.join("demo"), &mut sources);
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        let status = Command::new(javac)
            .args(["--release", "21", "-nowarn", "-d"])
            .arg(&classes)
            .args(&sources)
            .status()
            .expect("javac runs");
        assert!(status.success(), "stream-codecs fixture javac failed");

        let mut bytes = Vec::new();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        let mut class_files: Vec<PathBuf> = std::fs::read_dir(classes.join("demo"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        class_files.sort();
        for path in class_files {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            zip.start_file(format!("demo/{name}"), options).unwrap();
            zip.write_all(&std::fs::read(path).unwrap()).unwrap();
        }
        zip.finish().unwrap();
        let _ = std::fs::remove_dir_all(&scratch);
        NativeJar::from_bytes(&bytes).unwrap()
    })
}

#[test]
fn constructions_match_java_25_classes_and_identity() {
    let mut host = StdHost::new();
    let answer = jar().invoke_method_with_args(
        "demo/StreamCodecs",
        "constructions",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert!(
        matches!(answer, Ok(Value::Ref(Some(ref text))) if text == "net.minecraft.network.codec.StreamCodec$7|true|true|net.minecraft.network.codec.StreamCodec$8|true|true|net.minecraft.network.codec.StreamCodec$2|true|true|net.minecraft.network.protocol.common.custom.CustomPacketPayload$1|true|true|net.minecraft.network.codec.StreamCodec$1|true|true|net.minecraft.network.codec.StreamCodec$4|true|true|true|false|true|false"),
        "{answer:?}"
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn codec_members_remain_counted() {
    let mut host = StdHost::new();
    let answer = jar().invoke_method_with_args(
        "demo/StreamCodecs",
        "countedMembers",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert!(
        matches!(answer, Ok(Value::Ref(Some(ref text))) if text == "done"),
        "{answer:?}"
    );
    for name in ["toString", "hashCode", "apply", "dispatch"] {
        assert!(
            host.unknown_host_calls()
                .iter()
                .any(|(call, _)| call.contains(&format!("#{name}"))),
            "{name}: {:?}",
            host.unknown_host_calls()
        );
    }
}

#[test]
fn stream_codec_string_forgery_stays_a_string() {
    use crate::Host;
    let mut host = StdHost::new();
    let codec = crate::stream_codec::static_call(
        &crate::MethodRef {
            owner: crate::stream_codec::STREAM_CODEC.into(),
            name: "composite".into(),
            descriptor: "(Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Ljava/util/function/Function;)Lnet/minecraft/network/codec/StreamCodec;".into(),
        },
        &[Value::Ref(None), Value::Ref(None), Value::Ref(None)],
    ).unwrap();
    let key = match codec {
        Value::Host(ref handle) => handle.key().to_owned(),
        other => panic!("expected host codec, got {other:?}"),
    };
    let forged = Value::Ref(Some(key));
    let call = crate::MethodRef {
        owner: crate::stream_codec::STREAM_CODEC.into(),
        name: "map".into(),
        descriptor: "(Ljava/util/function/Function;Ljava/util/function/Function;)Lnet/minecraft/network/codec/StreamCodec;".into(),
    };
    let _ = host.invoke_instance(&call, &forged, &[Value::Ref(None), Value::Ref(None)]);
    assert!(
        host.unknown_host_calls()
            .iter()
            .any(|(entry, _)| entry.contains("#map"))
    );
}

#[test]
fn codec_construction_retains_callback_handles() {
    let host = StdHost::new();
    let callback = Value::Host(crate::host_ref::HostRef::new(
        crate::HostKind::JarObject,
        "callback#lambda",
    ));
    let codec = crate::stream_codec::static_call(
        &crate::MethodRef {
            owner: crate::stream_codec::STREAM_CODEC.into(),
            name: "of".into(),
            descriptor: "(Lnet/minecraft/network/codec/StreamEncoder;Lnet/minecraft/network/codec/StreamDecoder;)Lnet/minecraft/network/codec/StreamCodec;".into(),
        },
        &[callback.clone(), callback.clone()],
    )
    .unwrap();
    let values = match codec {
        Value::Host(handle) => handle.values().unwrap(),
        other => panic!("expected host codec, got {other:?}"),
    };
    assert!(
        values[2..]
            .iter()
            .all(|value| matches!(value, Value::Host(_)))
    );
    let _ = host;
}

#[test]
fn network_methods_remain_counted() {
    let mut host = StdHost::new();
    let answer = jar().invoke_method_with_args(
        "demo/StreamCodecs",
        "encodeDecode",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    // Java 25 oracle: encodeDecode=java.lang.NullPointerException. The
    // encode/decode calls are intentionally COUNTED, so do not pin the
    // interpreter's neutral return value as an answer.
    let _ = answer;
    let unknown = host.unknown_host_calls();
    assert!(
        unknown.iter().any(|(call, _)| call.contains("#encode")),
        "{unknown:?}"
    );
    assert!(
        unknown.iter().any(|(call, _)| call.contains("#decode")),
        "{unknown:?}"
    );
}
