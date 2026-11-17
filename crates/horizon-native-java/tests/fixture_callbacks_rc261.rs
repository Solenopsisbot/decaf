//! The #261 re-review's probe (`fixtures/callbacks_rc261/demo/Rv.java`, as
//! written; `java25.txt` is its output on OpenJDK 25.0.5+8), and every text
//! a jar can make from a callback (`fixtures/callbacks_leaks/`).
//!
//! * B1: a comparator (`comparing`, `reversed`, `thenComparing`) and a map
//!   entry holding a lambda are handles, so a cast to `CharSequence`/`String`
//!   is Java's CCE and nothing can be read out of them; no text holds a
//!   callback's key (the leak scan);
//! * B2: a `String` spelled like a comparator (`@cmp:…`) is a `String`: no
//!   cast makes it a `Comparator`, no compare reads handles out of it, and
//!   `equals`/`hashCode`/`getClass` are a `String`'s;
//! * a lambda is one object per `invokedynamic` instruction (`finally`
//!   bodies javac copies), per evaluation when it captures; composites run
//!   in Java's order; a lambda comparator sorts as `TimSort` does;
//! * what a callback holds survives collections (`gcHold`, 40 garbage
//!   calls, `gcUse`).
//!
//! Every case's answer and counted calls are pinned. The answers that are
//! not Java's are named below; all are counted.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const PROBE: &str = include_str!("fixtures/callbacks_rc261/demo/Rv.java");
const ORACLE: &str = include_str!("fixtures/callbacks_rc261/java25.txt");
const LEAKS: &str = include_str!("fixtures/callbacks_leaks/demo/Leaks.java");
const LEAKS_JAVA: &str = include_str!("fixtures/callbacks_leaks/java25.txt");

/// The calls a case counts. Every other case counts nothing.
const COUNTED: &[(&str, &[&str])] = &[
    (
        "cmpKeyCastCharSeq",
        &["java/lang/CharSequence#<checkcast>(Ljava/lang/Object;)Ljava/lang/CharSequence;"],
    ),
    (
        "cmpReversedCastString",
        &["java/lang/String#<checkcast>(Ljava/lang/Object;)Ljava/lang/String;"],
    ),
    (
        "cmpThenCastCharSeq",
        &["java/lang/CharSequence#<checkcast>(Ljava/lang/Object;)Ljava/lang/CharSequence;"],
    ),
    (
        "cmpConcatLeak",
        &["java/lang/Object#toString()Ljava/lang/String;"],
    ),
    (
        "cmpEqualsText",
        &["java/lang/String#contentEquals(Ljava/lang/CharSequence;)Z"],
    ),
    ("cmpHashCodeText", &["java/lang/Object#hashCode()I"]),
    (
        "cmpArraysSort",
        &[
            "java/util/Arrays#sort([Ljava/lang/Object;Ljava/util/Comparator;)V",
            "java/util/Arrays#toString([Ljava/lang/Object;)Ljava/lang/String;",
        ],
    ),
    (
        "cmpMax",
        &[
            "java/util/Collections#max(Ljava/util/Collection;Ljava/util/Comparator;)Ljava/lang/Object;",
        ],
    ),
    (
        "cmpModifiesList",
        &["java/util/List#sort(Ljava/util/Comparator;)V"],
    ),
];

/// The cases whose answer is not Java's, and why.
const DEPARTURES: &[(&str, &str)] = &[
    // `String.contentEquals` is not modelled: counted.
    ("cmpEqualsText", "false,false,false,false"),
    // A comparator's `hashCode` is its identity hash: counted, so two
    // answer alike.
    ("cmpHashCodeText", "true,false,false"),
    // `Arrays.sort(T[], Comparator)` is not modelled: counted.
    ("cmpArraysSort", "null|"),
    // `Collections.max(coll, Comparator)` is not modelled: counted.
    ("cmpMax", "null,ccc,a|"),
];

fn shown(out: Result<Value, horizon_native_java::NativeError>) -> String {
    match out {
        Ok(Value::Ref(Some(text))) => text,
        other => format!("{other:?}"),
    }
}

#[test]
fn every_rc261_case_answers_as_java_or_is_named() {
    let fixture = javac_fixture::Fixture::build("callbacks-rc261", &[("demo/Rv.java", PROBE)]);
    let lookup = |method: &str| {
        COUNTED
            .iter()
            .find(|(name, _)| *name == method)
            .map_or(&[][..], |(_, calls)| *calls)
    };
    let mut wrong = Vec::new();
    for line in ORACLE.lines().filter(|line| !line.starts_with("gc")) {
        let (method, java) = line.split_once('=').expect("case=answer");
        let mut host = fixture.jar.seed_host(StdHost::new());
        let shown = shown(fixture.jar.invoke_method_with_args(
            "demo/Rv",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        ));
        let expected = DEPARTURES
            .iter()
            .find(|(name, _)| *name == method)
            .map_or(java, |(_, ours)| ours);
        let counted: Vec<String> = host
            .unknown_host_calls()
            .into_iter()
            .map(|(call, _)| call)
            .collect();
        if shown != expected || counted != lookup(method) {
            wrong.push(format!(
                "{method}: {shown} (want {expected}), counted {counted:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// `gcHold` keeps callbacks that only other callbacks, collections and a
/// comparator reach; 40 calls of garbage make the host collect; `gcUse`
/// reads them back.
#[test]
fn what_callbacks_hold_survives_collections() {
    let fixture = javac_fixture::Fixture::build("callbacks-rc261-gc", &[("demo/Rv.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let call = |method: &str, host: &mut StdHost| {
        shown(fixture.jar.invoke_method_with_args(
            "demo/Rv",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            host,
        ))
    };
    let java = |key: &str| {
        ORACLE
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .unwrap()
            .to_owned()
    };
    assert_eq!(call("gcHold", &mut host), java("gcHold="));
    let before = host.resident_heap().instance_fields();
    let mut collected = false;
    for _ in 0..40 {
        call("gcGarbage", &mut host);
        collected |= host.resident_heap().instance_fields() <= before;
    }
    assert!(collected, "the host never collected");
    assert_eq!(call("gcUse", &mut host), java("gcUse="));
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}

/// No text a jar can make from a callback, a comparator or a structure
/// holding one (`String.valueOf`, concatenation, `StringBuilder`, `format`,
/// a collection's, map's, array's or optional's `toString`, `String.join`)
/// names a callback's key.
#[test]
fn no_text_a_jar_makes_names_a_callback() {
    let fixture = javac_fixture::Fixture::build("callbacks-leaks", &[("demo/Leaks.java", LEAKS)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        shown(fixture.jar.invoke_method_with_args(
            "demo/Leaks",
            "run",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )),
        LEAKS_JAVA.trim_end()
    );
}
