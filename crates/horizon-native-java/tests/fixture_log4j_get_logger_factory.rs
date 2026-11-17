//! `LogManager.getLogger(String, MessageFactory)` with a mod's own factory (`jdk_model.rs`,
//! `host_jdk.rs`).
//!
//! `fixtures/log4j_get_logger_factory/demo/Probe.java` runs on Java 25 with the Log4j 2.26.0 the game
//! ships (command in the README), and here against compile-only stubs: only `demo/` ships. Log4j
//! caches a logger per (factory, name), and a factory extending `AbstractMessageFactory` or
//! `FormattedMessageFactory` comes back from `getMessageFactory()` as itself, so `rows()` must equal
//! `java25.txt` with nothing counted. A plain `MessageFactory` is wrapped in an adapter the host
//! doesn't hold: `plainFactory` counts its `getMessageFactory()` once (`java25_counted.txt`).
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/log4j_get_logger_factory/demo/Probe.java");
const JAVA25_CAPTURE: &str = include_str!("fixtures/log4j_get_logger_factory/java25.txt");
const JAVA25_COUNTED: &str = include_str!("fixtures/log4j_get_logger_factory/java25_counted.txt");

const STUBS: &[(&str, &str)] = &[
    (
        "org/apache/logging/log4j/Logger.java",
        "package org.apache.logging.log4j; import org.apache.logging.log4j.message.MessageFactory; public interface Logger { String getName(); <MF extends MessageFactory> MF getMessageFactory(); void info(String m, Object p); boolean isDebugEnabled(); }",
    ),
    (
        "org/apache/logging/log4j/LogManager.java",
        "package org.apache.logging.log4j; import org.apache.logging.log4j.message.MessageFactory; public class LogManager { public static Logger getLogger(String n, MessageFactory f) { throw new AssertionError(); } public static Logger getLogger(String n) { throw new AssertionError(); } }",
    ),
    (
        "org/apache/logging/log4j/message/Message.java",
        "package org.apache.logging.log4j.message; public interface Message {}",
    ),
    (
        "org/apache/logging/log4j/message/SimpleMessage.java",
        "package org.apache.logging.log4j.message; public class SimpleMessage implements Message { public SimpleMessage(String s) {} }",
    ),
    (
        "org/apache/logging/log4j/message/ObjectMessage.java",
        "package org.apache.logging.log4j.message; public class ObjectMessage implements Message { public ObjectMessage(Object o) {} }",
    ),
    (
        "org/apache/logging/log4j/message/ParameterizedMessage.java",
        "package org.apache.logging.log4j.message; public class ParameterizedMessage implements Message { public ParameterizedMessage(String s, Object... a) {} }",
    ),
    (
        "org/apache/logging/log4j/message/MessageFactory.java",
        "package org.apache.logging.log4j.message; public interface MessageFactory { Message newMessage(Object o); Message newMessage(String s); Message newMessage(String s, Object... a); }",
    ),
    (
        "org/apache/logging/log4j/message/AbstractMessageFactory.java",
        "package org.apache.logging.log4j.message; public abstract class AbstractMessageFactory implements MessageFactory { public Message newMessage(Object o) { throw new AssertionError(); } public Message newMessage(String s) { throw new AssertionError(); } }",
    ),
    (
        "org/apache/logging/log4j/message/FormattedMessageFactory.java",
        "package org.apache.logging.log4j.message; public class FormattedMessageFactory implements MessageFactory { public Message newMessage(Object o) { throw new AssertionError(); } public Message newMessage(String s) { throw new AssertionError(); } public Message newMessage(String s, Object... a) { throw new AssertionError(); } }",
    ),
];

const GET_MESSAGE_FACTORY: &str = "org/apache/logging/log4j/Logger#getMessageFactory()Lorg/apache/logging/log4j/message/MessageFactory;";
const IS_DEBUG_ENABLED: &str = "org/apache/logging/log4j/Logger#isDebugEnabled()Z";

fn fixture() -> Fixture {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    Fixture::build("log4j-get-logger-factory", &sources)
}

#[test]
fn mod_factory_loggers_match_java25() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    let unknown = host.unknown_host_calls();
    assert_eq!(rows, JAVA25_CAPTURE, "unknown calls: {unknown:?}");
    assert_eq!(unknown, Vec::<(String, usize)>::new());
}

#[test]
fn a_plain_factory_is_counted() {
    let fixture = fixture();
    let cases = [
        ("plainFactory", GET_MESSAGE_FACTORY),
        ("levelCheck", IS_DEBUG_ENABLED),
    ];
    let captured: Vec<&str> = JAVA25_COUNTED
        .lines()
        .map(|line| line.split_once('=').expect("method=answer").0)
        .collect();
    assert_eq!(captured, cases.map(|(method, _)| method));
    for (method, counted) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        fixture.text_on(&mut host, method);
        assert_eq!(
            host.unknown_host_calls(),
            vec![(counted.to_owned(), 1)],
            "{method}"
        );
    }
}
