//! Equivalence tests for immutable `ClassFile` and `NativeJar` metadata memos.
//!
//! Each test compares the memoised path (both on first miss and on subsequent cross-runtime
//! hits) against the uncached computation across primary classes, nested-jar classes,
//! passthrough accessors, mixin-package classes, and classes that fail to load.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, OnceLock};

use crate::runtime::class_precedence::RunLoader;
use crate::runtime::interface_mixins::{RuntimeMixinState, SharedRuntimeCache};
use crate::runtime::mixin_class_loading::LoadFailure;
use crate::runtime::{
    ClassFile, Host, MixinSide, NativeJar, Reader, Runtime, StdHost, class_name, read_pool,
};

fn javac_bin() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| PathBuf::from("javac"))
}

fn compile_sources(tag: &str, sources: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    let scratch = std::env::temp_dir().join(format!(
        "horizon-native-java-memo-cache-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let src_dir = scratch.join("src");
    let out_dir = scratch.join("classes");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut paths = Vec::new();
    for (rel, body) in sources {
        let path = src_dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
        paths.push(path);
    }

    let status = Command::new(javac_bin())
        .args(["--release", "21", "-nowarn", "-d"])
        .arg(&out_dir)
        .args(&paths)
        .status()
        .expect("javac runs");
    assert!(status.success(), "javac failed for memo_cache_tests/{tag}");

    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|ext| ext == "class") {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, std::fs::read(&path).unwrap()));
            }
        }
    }

    let mut classes = Vec::new();
    walk(&out_dir, &out_dir, &mut classes);
    let _ = std::fs::remove_dir_all(&scratch);
    classes.sort_by(|a, b| a.0.cmp(&b.0));
    classes
}

fn pack_jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }
    bytes
}

struct FixtureJars {
    primary_bytes: Vec<u8>,
    nested_bytes: Vec<u8>,
}

fn fixture_jars() -> &'static FixtureJars {
    static JARS: OnceLock<FixtureJars> = OnceLock::new();
    JARS.get_or_init(|| {
        let compiled = compile_sources(
            "bundle",
            &[
                (
                    "org/spongepowered/asm/mixin/Mixin.java",
                    "package org.spongepowered.asm.mixin;\n\
                     import java.lang.annotation.*;\n\
                     @Retention(RetentionPolicy.CLASS)\n\
                     public @interface Mixin { Class<?>[] value() default {}; }\n",
                ),
                (
                    "org/spongepowered/asm/mixin/gen/Accessor.java",
                    "package org.spongepowered.asm.mixin.gen;\n\
                     import java.lang.annotation.*;\n\
                     @Retention(RetentionPolicy.RUNTIME)\n\
                     public @interface Accessor { String value() default \"\"; }\n",
                ),
                (
                    "nested/LibIface.java",
                    "package nested; public interface LibIface {}\n",
                ),
                (
                    "nested/LibSubIface.java",
                    "package nested; public interface LibSubIface extends LibIface {}\n",
                ),
                (
                    "nested/DuckIface.java",
                    "package nested; public interface DuckIface {}\n",
                ),
                (
                    "nested/LibBase.java",
                    "package nested; public class LibBase implements LibSubIface {}\n",
                ),
                (
                    "nested/LibChild.java",
                    "package nested; public class LibChild extends LibBase {}\n",
                ),
                (
                    "nested/mixin/NestedDuckMixin.java",
                    "package nested.mixin;\n\
                     import org.spongepowered.asm.mixin.Mixin;\n\
                     @Mixin(nested.LibBase.class)\n\
                     public abstract class NestedDuckMixin implements nested.DuckIface {}\n",
                ),
                (
                    "demo/WovenDuck.java",
                    "package demo; public interface WovenDuck {}\n",
                ),
                (
                    "demo/BrokenTarget.java",
                    "package demo; public class BrokenTarget {}\n",
                ),
                (
                    "demo/SubBroken.java",
                    "package demo; public class SubBroken extends BrokenTarget {}\n",
                ),
                (
                    "demo/mixin/Secret.java",
                    "package demo.mixin; public class Secret {}\n",
                ),
                (
                    "demo/mixin/AccMixin.java",
                    "package demo.mixin;\n\
                     import org.spongepowered.asm.mixin.Mixin;\n\
                     import org.spongepowered.asm.mixin.gen.Accessor;\n\
                     @Mixin(demo.Normal.class)\n\
                     public interface AccMixin { @Accessor int getTag(); }\n",
                ),
                (
                    "demo/mixin/BrokenMixin.java",
                    "package demo.mixin;\n\
                     import org.spongepowered.asm.mixin.Mixin;\n\
                     @Mixin(demo.BrokenTarget.class)\n\
                     public abstract class BrokenMixin { public static int BAD = 1; }\n",
                ),
                (
                    "demo/mixin/DuckMixin.java",
                    "package demo.mixin;\n\
                     import org.spongepowered.asm.mixin.Mixin;\n\
                     @Mixin(demo.Normal.class)\n\
                     public abstract class DuckMixin implements demo.WovenDuck {}\n",
                ),
                (
                    "demo/Normal.java",
                    "package demo;\n\
                     import java.io.Serializable;\n\
                     public class Normal extends nested.LibChild implements Serializable {\n\
                       private int tag;\n\
                       public static Class<?> refSecret() { return demo.mixin.Secret.class; }\n\
                       public static Class<?> refBroken() { return BrokenTarget.class; }\n\
                       public static Class<?> refAcc() { return demo.mixin.AccMixin.class; }\n\
                       public static Class<?> refNested() { return nested.LibChild.class; }\n\
                       public int catchProbe(int x) {\n\
                         try { return 10 / x; } catch (ArithmeticException e) { return -1; }\n\
                       }\n\
                     }\n",
                ),
                (
                    "demo/Clean.java",
                    "package demo;\n\
                     public final class Clean {\n\
                       public static Class<?> refNested() { return nested.LibBase.class; }\n\
                     }\n",
                ),
            ],
        );

        let nested_mod_json = br#"{"schemaVersion":1,"id":"nestedlib","version":"1.0.0","mixins":["nested.mixins.json"]}"#;
        let nested_mixin_json = br#"{"required":false,"package":"nested.mixin","mixins":["NestedDuckMixin"]}"#;
        let mut nested_entries: Vec<(&str, &[u8])> = vec![
            ("fabric.mod.json", nested_mod_json),
            ("nested.mixins.json", nested_mixin_json),
        ];
        for (name, bytes) in &compiled {
            if name.starts_with("nested/") {
                nested_entries.push((name.as_str(), bytes.as_slice()));
            }
        }
        let nested_bytes = pack_jar(&nested_entries);

        let primary_mod_json = br#"{
            "schemaVersion": 1,
            "id": "memodemo",
            "version": "1.0.0",
            "mixins": [
                "demo.mixins.json",
                {"config": "demo.client.mixins.json", "environment": "client"}
            ],
            "jars": [{"file": "META-INF/jars/nested-lib.jar"}]
        }"#;
        let primary_mixin_json = br#"{
            "required": true,
            "package": "demo.mixin",
            "mixins": ["BrokenMixin", "AccMixin", "DuckMixin"]
        }"#;
        let primary_client_mixin_json = br#"{
            "required": false,
            "package": "demo.clientmixin",
            "mixins": []
        }"#;
        let mut primary_entries: Vec<(&str, &[u8])> = vec![
            ("fabric.mod.json", primary_mod_json),
            ("demo.mixins.json", primary_mixin_json),
            ("demo.client.mixins.json", primary_client_mixin_json),
            ("META-INF/jars/nested-lib.jar", &nested_bytes),
        ];
        for (name, bytes) in &compiled {
            if name.starts_with("demo/") {
                primary_entries.push((name.as_str(), bytes.as_slice()));
            }
        }
        let primary_bytes = pack_jar(&primary_entries);

        FixtureJars {
            primary_bytes,
            nested_bytes,
        }
    })
}

fn uncached_header_from_raw(file: &ClassFile) -> (Option<u16>, Vec<String>) {
    if file.raw.is_empty() {
        return (None, Vec::new());
    }
    let mut r = Reader::new(&file.raw);
    assert_eq!(r.u4().unwrap(), 0xCAFE_BABE);
    r.skip(4).unwrap();
    let pool = read_pool(&mut r).unwrap();
    let access = r.u2().unwrap();
    let _this = r.u2().unwrap();
    let _super = r.u2().unwrap();
    let count = r.u2().unwrap() as usize;
    let mut interfaces = Vec::with_capacity(count);
    for _ in 0..count {
        interfaces.push(class_name(&pool, r.u2().unwrap()).unwrap());
    }
    (Some(access), interfaces)
}

fn with_test_runtime<R>(
    jar: &NativeJar,
    host: &mut StdHost,
    uncached: bool,
    f: impl FnOnce(&Runtime<'_, StdHost>) -> R,
) -> R {
    let mut mixin_index = RuntimeMixinState::from_jar(jar, host);
    if uncached {
        mixin_index.cache = Arc::new(SharedRuntimeCache::default());
    }
    let linked = host.linked_class_table().cloned().unwrap_or_default();
    let mut callback_infos = crate::runtime::CallbackInfos::new();
    let runtime = Runtime {
        classes: &jar.classes,
        mixin_index,
        linked,
        host,
        calls: 0,
        instructions: 0,
        fields: Default::default(),
        static_fields: Default::default(),
        initialized_classes: Default::default(),
        arrays: Default::default(),
        next_array: 0,
        next_object: 0,
        streams: Default::default(),
        next_stream: 0,
        pipelines: Default::default(),
        enum_constants: Default::default(),
        string_builders: Default::default(),
        counted_strings: Default::default(),
        identity_collections: Default::default(),
        string_pool: Default::default(),
        counted_text_objects: Default::default(),
        gson_writers: Default::default(),
        string_builder_capacities: Default::default(),
        string_readers: Default::default(),
        reader_positions_unknown: Default::default(),
        io_reader_errors: Default::default(),
        scanners: Default::default(),
        io_stream_paths: Default::default(),
        io_channel_streams: Default::default(),
        io_writers: Default::default(),
        io_closed: Default::default(),
        io_writer_bytes: Default::default(),
        io_stream_positions: Default::default(),
        io_writer_charsets: Default::default(),
        io_writer_sinks: Default::default(),
        io_paths: Default::default(),
        io_files: Default::default(),
        io_file_bytes: Default::default(),
        io_dirs: Default::default(),
        watch: Default::default(),
        annotations: Default::default(),
        next_annotation: 0,
        callback_infos: &mut callback_infos,
        invoke_hooks: None,
        anchor_hooks: None,
        modify_hooks: None,
        pending_skipped_new: Vec::new(),
        stream_plan: Default::default(),
        store_verdict: Default::default(),
        depth: 0,
        next_frame_id: 0,
        shape_extraction: None,
        check_shape_probes: false,
        heap_flags: 0,
        budget: &crate::runtime::ExecutionBudget::UNBOUNDED,
    };
    f(&runtime)
}

#[test]
fn class_file_parsed_metadata_matches_raw_read_pool() {
    let jars = fixture_jars();
    let primary = NativeJar::from_bytes(&jars.primary_bytes).unwrap();
    let nested = NativeJar::from_bytes(&jars.nested_bytes).unwrap();

    for class in primary.classes.values().chain(nested.classes.values()) {
        let (expected_access, expected_ifaces) = uncached_header_from_raw(class);
        assert_eq!(
            class.access_flags(),
            expected_access,
            "access_flags mismatch for {}",
            class.name
        );
        assert_eq!(
            class.interface_names(),
            expected_ifaces,
            "interface_names mismatch for {}",
            class.name
        );
        assert_eq!(
            class.interfaces(),
            expected_ifaces.as_slice(),
            "interfaces slice mismatch for {}",
            class.name
        );
        // Verify cached constant-pool refs match uncached resolution for every pool slot.
        for (idx, entry) in class.pool.iter().enumerate() {
            let slot = idx as u16;
            match entry {
                crate::runtime::Cp::MethodRef { .. } | crate::runtime::Cp::InterfaceMethodRef { .. } => {
                    assert_eq!(
                        class.method_ref_cached(slot).unwrap(),
                        &class.method_ref(slot).unwrap(),
                        "method_ref_cached mismatch for {}#{slot}",
                        class.name
                    );
                }
                crate::runtime::Cp::FieldRef { .. } => {
                    assert_eq!(
                        class.field_ref_cached(slot).unwrap(),
                        &class.field_ref(slot).unwrap(),
                        "field_ref_cached mismatch for {}#{slot}",
                        class.name
                    );
                }
                _ => {}
            }
        }
    }

    // Verify exception handlers and member annotations read via `reader_after_pool`.
    let normal = primary.classes.get("demo/Normal").unwrap();
    let handlers = normal.exception_handlers("catchProbe", "(I)I").unwrap();
    assert_eq!(handlers.len(), 1);
    assert_eq!(
        handlers[0].catch_type.as_deref(),
        Some("java/lang/ArithmeticException")
    );
    let acc = primary.classes.get("demo/mixin/AccMixin").unwrap();
    let (_, method_anns) = crate::runtime::game_hooks::member_annotations(acc).unwrap();
    assert!(
        method_anns
            .get("getTag|()I")
            .is_some_and(|list| list.iter().any(|d| d == crate::runtime::MIXIN_ACCESSOR_DESCRIPTOR))
    );
}

#[test]
fn jar_mixin_configs_and_sides_match_uncached() {
    let jars = fixture_jars();
    let jar = NativeJar::from_bytes(&jars.primary_bytes).unwrap();

    let (uncached_configs, uncached_errors) = jar.mixin_config_scan();
    let uncached_sides = jar.compute_mixin_config_sides();

    // First call populates OnceLock; second hits the memo.
    assert_eq!(jar.mixin_configs(), uncached_configs);
    assert_eq!(jar.mixin_configs(), uncached_configs);
    assert_eq!(jar.mixin_config_errors(), uncached_errors);
    assert_eq!(jar.mixin_config_sides(), uncached_sides);
    assert_eq!(jar.mixin_config_sides(), uncached_sides);
    assert!(Arc::ptr_eq(
        &jar.cached_mixin_configs_arc(),
        &jar.cached_mixin_configs_arc()
    ));
    assert!(Arc::ptr_eq(
        &jar.cached_mixin_config_sides_arc(),
        &jar.cached_mixin_config_sides_arc()
    ));

    // `with_woven_classes` must reset the jar's caches to a fresh instance.
    let before_cache = Arc::clone(&jar.meta_cache);
    let woven = jar
        .clone()
        .with_woven_classes(&crate::runtime::WovenClasses::default())
        .unwrap();
    assert!(!Arc::ptr_eq(&before_cache, &woven.meta_cache));
    assert_eq!(woven.mixin_configs(), uncached_configs);
}

#[test]
fn runtime_caches_match_uncached_across_primary_nested_and_failing_classes() {
    let jars = fixture_jars();
    let jar = NativeJar::from_bytes(&jars.primary_bytes).unwrap();
    let mut host = jar
        .try_seed_host(StdHost::new().with_environment_type("CLIENT"))
        .unwrap();

    let classes = [
        "demo/Normal",
        "demo/Normal#1",
        "demo/Normal#2",
        "demo/Clean",
        "demo/BrokenTarget",
        "demo/SubBroken",
        "demo/WovenDuck",
        "demo/mixin/Secret",
        "demo/mixin/AccMixin",
        "demo/mixin/BrokenMixin",
        "demo/mixin/DuckMixin",
        "nested/LibBase",
        "nested/LibChild",
        "nested/LibIface",
        "nested/LibSubIface",
        "nested/DuckIface",
        "java/lang/Object",
    ];
    let wanted_types = [
        "demo/Normal",
        "demo/BrokenTarget",
        "demo/WovenDuck",
        "nested/LibBase",
        "nested/LibChild",
        "nested/LibIface",
        "nested/LibSubIface",
        "nested/DuckIface",
        "java/io/Serializable",
        "java/lang/Object",
        "java/lang/String",
    ];

    // 1. First runtime populates the shared per-jar cache; second runtime hits it;
    //    third runtime computes every answer uncached against a fresh cache.
    let mut first_instance = Vec::new();
    let mut first_unloadable = Vec::new();
    let mut first_failure = Vec::new();
    let mut first_pools = Vec::new();
    let first_index_ptr = with_test_runtime(&jar, &mut host, false, |rt| {
        for &src in &classes {
            for &dst in &wanted_types {
                first_instance.push((src, dst, rt.class_is_instance(src, dst)));
            }
            first_unloadable.push((src, rt.is_unloadable_by_name(src)));
            first_failure.push((src, rt.class_load_failure(src, 0)));
            if let Some(file) = rt.class(src) {
                first_pools.push((src, rt.unloadable_pool_entries(src, file)));
            }
        }
        rt.mixins_targeting("demo/Normal").as_ptr() as usize
    });

    // Sanity-check key semantic verdicts on failing, mixin-package, passthrough, and nested classes.
    assert!(
        first_unloadable
            .iter()
            .any(|&(c, v)| c == "demo/mixin/Secret" && v),
        "mixin-package class must be unloadable"
    );
    assert!(
        first_unloadable
            .iter()
            .any(|&(c, v)| c == "demo/mixin/AccMixin" && !v),
        "passthrough accessor must stay loadable"
    );
    assert!(
        first_failure
            .iter()
            .any(|&(c, v)| c == "demo/BrokenTarget" && v == LoadFailure::Fails),
        "required rejected mixin target must fail to load"
    );
    assert!(
        first_failure
            .iter()
            .any(|&(c, v)| c == "demo/SubBroken" && v == LoadFailure::Fails),
        "subclass of failing class must fail to load"
    );
    assert!(
        first_failure
            .iter()
            .any(|&(c, v)| c == "nested/LibChild" && v == LoadFailure::Loads),
        "nested jar class must load"
    );
    assert!(
        first_instance
            .iter()
            .any(|&(s, d, v)| s == "demo/Normal#1" && d == "nested/LibIface" && v),
        "primary subclass must transitively implement nested jar's superinterface"
    );
    assert!(
        first_pools
            .iter()
            .any(|(c, p)| *c == "demo/Normal" && p.as_ref().is_some_and(|s| !s.is_empty())),
        "demo/Normal pool must flag unloadable entries"
    );
    assert!(
        first_pools
            .iter()
            .any(|(c, p)| *c == "demo/Clean" && p.is_none()),
        "demo/Clean pool must have no unloadable entries"
    );

    // Second runtime (shared cache hit): must reuse the exact same RuntimeMixinIndex and match all answers.
    let second_index_ptr = with_test_runtime(&jar, &mut host, false, |rt| {
        for &(src, dst, expected) in &first_instance {
            assert_eq!(
                rt.class_is_instance(src, dst),
                expected,
                "cached hit class_is_instance({src}, {dst})"
            );
            assert_eq!(
                rt.class_is_instance_uncached(src, dst),
                expected,
                "uncached class_is_instance({src}, {dst})"
            );
        }
        for &(src, expected) in &first_unloadable {
            assert_eq!(
                rt.is_unloadable_by_name(src),
                expected,
                "cached hit is_unloadable_by_name({src})"
            );
        }
        for &(src, expected) in &first_failure {
            assert_eq!(
                rt.class_load_failure(src, 0),
                expected,
                "cached hit class_load_failure({src})"
            );
        }
        for (src, expected) in &first_pools {
            let file = rt.class(src).unwrap();
            assert_eq!(
                rt.unloadable_pool_entries(src, file).as_deref(),
                expected.as_deref(),
                "cached hit unloadable_pool_entries({src})"
            );
        }
        // Check mixins targeting both primary and nested jar classes.
        assert!(rt.mixin_adds_interface("demo/Normal", "demo/WovenDuck"));
        assert!(rt.mixin_adds_interface("nested/LibBase", "nested/DuckIface"));
        // Verify member_owner and field_owner caches match uncached resolution across primary and nested classes.
        for &(cls, mname, mdesc) in &[
            ("demo/Normal", "catchProbe", "(I)I"),
            ("demo/Normal", "toString", "()Ljava/lang/String;"),
            ("nested/LibChild", "baseMethod", "()I"),
            ("demo/SubBroken", "broken", "()V"),
        ] {
            let cached = rt.member_owner(cls, mname, mdesc);
            assert_eq!(rt.member_owner(cls, mname, mdesc), cached);
            assert_eq!(rt.member_owner_uncached(cls, mname, mdesc), cached);
        }
        for &(owner, fname, fdesc) in &[
            ("demo/Normal", "tag", "I"),
            ("nested/LibChild", "sharedField", "I"),
            ("demo/SubBroken", "missing", "I"),
        ] {
            let fref = crate::runtime::MethodRef {
                owner: owner.into(),
                name: fname.into(),
                descriptor: fdesc.into(),
            };
            let cached = rt.field_owner(&fref);
            assert_eq!(rt.field_owner(&fref), cached);
            assert_eq!(rt.field_owner_uncached(&fref), cached);
        }
        rt.mixins_targeting("demo/Normal").as_ptr() as usize
    });
    assert_eq!(
        first_index_ptr, second_index_ptr,
        "second runtime must reuse the shared RuntimeMixinIndex"
    );

    // Third runtime with a fresh uncached `SharedRuntimeCache`: every answer must match.
    with_test_runtime(&jar, &mut host, true, |rt| {
        for &(src, dst, expected) in &first_instance {
            assert_eq!(
                rt.class_is_instance(src, dst),
                expected,
                "fresh uncached class_is_instance({src}, {dst})"
            );
        }
        for &(src, expected) in &first_unloadable {
            assert_eq!(
                rt.is_unloadable_by_name(src),
                expected,
                "fresh uncached is_unloadable_by_name({src})"
            );
        }
        for &(src, expected) in &first_failure {
            assert_eq!(
                rt.class_load_failure(src, 0),
                expected,
                "fresh uncached class_load_failure({src})"
            );
        }
        for (src, expected) in &first_pools {
            let file = rt.class(src).unwrap();
            assert_eq!(
                rt.unloadable_pool_entries(src, file).as_deref(),
                expected.as_deref(),
                "fresh uncached unloadable_pool_entries({src})"
            );
        }
        assert!(rt.mixin_adds_interface("demo/Normal", "demo/WovenDuck"));
        assert!(rt.mixin_adds_interface("nested/LibBase", "nested/DuckIface"));
    });

    // Verify cache isolation across side, linked table, and run loader.
    let mut server_host = jar
        .try_seed_host(StdHost::new().with_environment_type("SERVER"))
        .unwrap();
    let mut unlinked_host = StdHost::new()
        .with_environment_type("CLIENT")
        .with_run_loader(RunLoader::Fabric);
    let mut neoforge_host = host.clone().with_run_loader(RunLoader::NeoForge);

    let client_cache = RuntimeMixinState::from_jar(&jar, &host).cache;
    let server_cache = RuntimeMixinState::from_jar(&jar, &server_host).cache;
    let unlinked_cache = RuntimeMixinState::from_jar(&jar, &unlinked_host).cache;
    let neoforge_cache = RuntimeMixinState::from_jar(&jar, &neoforge_host).cache;

    assert!(!Arc::ptr_eq(&client_cache, &server_cache));
    assert!(!Arc::ptr_eq(&client_cache, &unlinked_cache));
    assert!(!Arc::ptr_eq(&client_cache, &neoforge_cache));

    // Unlinked host does NOT see `nested/LibIface`, proving no cross-host cache bleed.
    with_test_runtime(&jar, &mut unlinked_host, false, |rt| {
        assert!(!rt.class_is_instance("demo/Normal", "nested/LibIface"));
        assert_eq!(
            rt.class_is_instance("demo/Normal", "nested/LibIface"),
            rt.class_is_instance_uncached("demo/Normal", "nested/LibIface")
        );
    });
    // Server host sees only non-client mixin packages (`demo.clientmixin` is client-only).
    with_test_runtime(&jar, &mut server_host, false, |rt| {
        assert!(!rt.is_unloadable_by_name("demo/clientmixin/OnlyClient"));
    });
    with_test_runtime(&jar, &mut host, false, |rt| {
        assert!(rt.is_unloadable_by_name("demo/clientmixin/OnlyClient"));
    });
    let _ = &mut neoforge_host;

    // Tooltip create callbacks memo matches uncached computation.
    let expected_cb = jar.compute_tooltip_create_callbacks("CLIENT", MixinSide::Client);
    assert_eq!(jar.tooltip_create_callbacks(&host), expected_cb);
    assert_eq!(jar.tooltip_create_callbacks(&host), expected_cb);
}

#[test]
fn linked_table_mutation_invalidates_shared_runtime_cache() {
    let jars = fixture_jars();
    let primary = NativeJar::from_bytes(&jars.primary_bytes).unwrap();
    let nested = NativeJar::from_bytes(&jars.nested_bytes).unwrap();

    // 1. Mutate the SAME host's linked table via `link_classes`, then `link_jar`.
    let mut host = StdHost::new().with_environment_type("CLIENT");
    with_test_runtime(&primary, &mut host, false, |rt| {
        assert!(!rt.class_is_instance("demo/Normal", "nested/LibIface"));
        assert!(!rt.interface_extends("nested/LibSubIface", "nested/LibIface"));
        assert!(!rt.mixin_adds_interface("nested/LibBase", "nested/DuckIface"));
        assert!(!rt.is_unloadable_by_name("nested/mixin/NestedDuckMixin"));
    });

    host.link_classes("nested-classes", nested.classes.values().cloned())
        .unwrap();
    with_test_runtime(&primary, &mut host, false, |rt| {
        assert!(
            rt.class_is_instance("demo/Normal", "nested/LibIface"),
            "link_classes must invalidate cached class_is_instance negative"
        );
        assert!(
            rt.interface_extends("nested/LibSubIface", "nested/LibIface"),
            "link_classes must invalidate cached interface_extends negative"
        );
        // Mixin configs are not linked yet; populate fresh negatives under the bumped id.
        assert!(!rt.mixin_adds_interface("nested/LibBase", "nested/DuckIface"));
        assert!(!rt.is_unloadable_by_name("nested/mixin/NestedDuckMixin"));
    });

    host.link_jar("nested-jar", &nested).unwrap();
    with_test_runtime(&primary, &mut host, false, |rt| {
        assert!(
            rt.mixin_adds_interface("nested/LibBase", "nested/DuckIface"),
            "link_jar must invalidate cached RuntimeMixinIndex"
        );
        assert!(
            rt.is_unloadable_by_name("nested/mixin/NestedDuckMixin"),
            "link_jar must invalidate cached refused_mixin_package"
        );
    });

    // 2. Also mutate a fresh unlinked host directly via `link_jar` alone.
    let mut direct_host = StdHost::new().with_environment_type("CLIENT");
    with_test_runtime(&primary, &mut direct_host, false, |rt| {
        assert!(!rt.class_is_instance("demo/Normal", "nested/LibIface"));
        assert!(!rt.mixin_adds_interface("nested/LibBase", "nested/DuckIface"));
        assert!(!rt.is_unloadable_by_name("nested/mixin/NestedDuckMixin"));
    });
    direct_host.link_jar("nestedlib", &nested).unwrap();
    with_test_runtime(&primary, &mut direct_host, false, |rt| {
        assert!(
            rt.class_is_instance("demo/Normal", "nested/LibIface"),
            "direct link_jar must invalidate cached class_is_instance"
        );
        assert!(
            rt.mixin_adds_interface("nested/LibBase", "nested/DuckIface"),
            "direct link_jar must invalidate cached RuntimeMixinIndex"
        );
        assert!(
            rt.is_unloadable_by_name("nested/mixin/NestedDuckMixin"),
            "direct link_jar must invalidate cached refused_mixin_package"
        );
    });
}

#[test]
fn with_woven_classes_invalidates_cached_runtime_answers() {
    let original = compile_sources(
        "woven-before",
        &[
            (
                "woven/Marker.java",
                "package woven; public interface Marker {}\n",
            ),
            ("woven/Base.java", "package woven; public class Base {}\n"),
            (
                "woven/Child.java",
                "package woven; public class Child extends Base {}\n",
            ),
        ],
    );
    let replacement = compile_sources(
        "woven-after",
        &[
            (
                "woven/Marker.java",
                "package woven; public interface Marker {}\n",
            ),
            (
                "woven/Base.java",
                "package woven;\n\
                 public class Base implements Marker {\n\
                   public int wovenField;\n\
                   public static int wovenTag() { return 42; }\n\
                 }\n",
            ),
        ],
    );
    let replacement_base = replacement
        .into_iter()
        .find(|(name, _)| name == "woven/Base.class")
        .expect("replacement woven/Base.class")
        .1;

    let jar_entries: Vec<(&str, &[u8])> = original
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    let jar = NativeJar::from_bytes(&pack_jar(&jar_entries)).unwrap();
    let mut host = StdHost::new().with_environment_type("CLIENT");
    let field_ref = crate::runtime::MethodRef {
        owner: "woven/Child".into(),
        name: "wovenField".into(),
        descriptor: "I".into(),
    };

    // Fill the per-jar SharedRuntimeCache with pre-weave answers.
    with_test_runtime(&jar, &mut host, false, |rt| {
        assert!(!rt.class_is_instance("woven/Base", "woven/Marker"));
        assert!(!rt.class_is_instance("woven/Child", "woven/Marker"));
        assert_eq!(
            rt.member_owner("woven/Base", "wovenTag", "()I"),
            Some(("java/lang/Object".to_owned(), false))
        );
        assert_eq!(rt.field_owner(&field_ref), "woven/Child");
    });

    // Replace `woven/Base` through `with_woven_classes` and re-query on the same host.
    let woven = jar
        .with_woven_classes(&crate::runtime::WovenClasses {
            classes: std::collections::BTreeMap::from([(
                "woven/Base".to_owned(),
                replacement_base,
            )]),
            added: std::collections::BTreeMap::new(),
        })
        .unwrap();

    with_test_runtime(&woven, &mut host, false, |rt| {
        assert!(
            rt.class_is_instance("woven/Base", "woven/Marker"),
            "with_woven_classes must invalidate cached class_is_instance on replaced class"
        );
        assert!(
            rt.class_is_instance("woven/Child", "woven/Marker"),
            "with_woven_classes must invalidate cached class_is_instance on subclass"
        );
        assert_eq!(
            rt.member_owner("woven/Base", "wovenTag", "()I"),
            Some(("woven/Base".to_owned(), true)),
            "with_woven_classes must invalidate cached member_owner"
        );
        assert_eq!(
            rt.field_owner(&field_ref),
            "woven/Base",
            "with_woven_classes must invalidate cached field_owner"
        );
    });
}
