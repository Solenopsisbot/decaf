#[path = "support/javac_fixture.rs"] mod javac_fixture;
const PROBE: &str = include_str!("fixtures/enum_body_get_class/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/enum_body_get_class/java25.txt");
#[test] fn enum_body_get_class_matches_java25() { let f=javac_fixture::Fixture::build("enum-body-get-class", &[("demo/Probe.java", PROBE)]); let mut h=f.jar.seed_host(horizon_native_java::StdHost::new()); let v=f.jar.invoke_method_with_args("demo/Probe","rows","()Ljava/lang/String;",None,&[],&mut h).unwrap(); let got=match v { horizon_native_java::Value::Ref(Some(s))=>s, x=>format!("{x:?}")}; assert_eq!(got,ORACLE.trim()); assert!(h.unknown_host_calls().is_empty(),"{:?}",h.unknown_host_calls()); }
