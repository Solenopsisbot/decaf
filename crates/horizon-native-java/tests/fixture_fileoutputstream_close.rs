#[path = "support/javac_fixture.rs"] mod javac_fixture;
const PROBE:&str=include_str!("fixtures/fileoutputstream-close/demo/Probe.java");
const ORACLE:&str=include_str!("fixtures/fileoutputstream-close/java25.txt");
#[test] fn fileoutputstream_close_matches_java25(){ let f=javac_fixture::Fixture::build("fileoutputstream-close", &[("demo/Probe.java",PROBE)]); let mut h=f.jar.seed_host(horizon_native_java::StdHost::new()); let v=f.jar.invoke_method_with_args("demo/Probe","rows","()Ljava/lang/String;",None,&[],&mut h).unwrap(); let got=match v {horizon_native_java::Value::Ref(Some(s))=>s, x=>format!("{x:?}")}; assert_eq!(got,ORACLE.trim_end()); assert!(h.unknown_host_calls().is_empty(),"{:?}",h.unknown_host_calls()); }
