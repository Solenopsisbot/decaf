#[path="support/javac_fixture.rs"] mod javac_fixture; use horizon_native_java::StdHost; use javac_fixture::Fixture;
const PROBE:&str=include_str!("fixtures/uri_tourl/demo/Probe.java"); const ORACLE:&str=include_str!("fixtures/uri_tourl/java25.txt");
#[test] fn uri_tourl_rows(){let f=Fixture::build("uri-tourl",&[("demo/Probe.java",PROBE)]); let mut h=f.jar.seed_host(StdHost::new()); assert_eq!(f.text_on(&mut h,"rows"),ORACLE);}
