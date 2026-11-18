#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
const PROBE: &str = r#"package demo;
import java.util.*; import java.util.stream.*;
public class Probe {
 static String err(Runnable r){try{r.run();return "ok";}catch(Throwable e){return e.getClass().getName();}}
 static Map<String,Integer> map(){return Stream.of("a","b").collect(Collectors.toUnmodifiableMap(x->x,x->x.length()));}
 public static String rows(){Map<String,Integer> m=map(); StringBuilder s=new StringBuilder();
  try{Stream.of("x").collect(Collectors.toUnmodifiableMap(x->(String)null,x->1));s.append("no");}catch(Throwable e){s.append(e.getClass().getName());}
  s.append("|").append(err(()->m.put("x",1))).append("|").append(err(()->m.remove("a"))).append("|").append(err(m::clear));
  s.append("|").append(err(()->m.keySet().remove("a"))).append("|").append(err(()->m.values().remove(1))).append("|").append(err(()->m.entrySet().clear()));
  s.append("|").append(m.get("a")).append("|").append(m.size()); return s.toString();}
 public static String counted(){Map<String,Integer> m=map(); return m.toString()+"|"+m.keySet()+"|"+m.values()+"|"+m.entrySet();}
}
"#;
fn fixture(test: &str) -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(test, &[("demo/Probe.java", PROBE)])
}
#[test]
fn unmodifiable_map_rows_match_java25() {
    let f = fixture("unmodifiable-map");
    let mut h = f.jar.seed_host(StdHost::new());
    assert_eq!(
        f.text_on(&mut h, "rows"),
        include_str!("fixtures/unmodifiable_map/java25.txt").trim_end()
    );
}
#[test]
fn unmodifiable_map_order_is_counted() {
    let f = fixture("unmodifiable-map-counted");
    let mut h = f.jar.seed_host(StdHost::new());
    let _ = f.text_on(&mut h, "counted");
    assert_eq!(
        h.nondeterministic_markers(),
        vec!["java-random-immutable-iteration".to_owned()]
    );
}
