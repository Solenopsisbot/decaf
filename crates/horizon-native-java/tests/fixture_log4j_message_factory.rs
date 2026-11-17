#[path = "support/javac_fixture.rs"] mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const LM:&str=r#"package org.apache.logging.log4j; import org.apache.logging.log4j.message.MessageFactory; public final class LogManager { public static Logger getLogger(String n){throw new AssertionError();} public static Logger getLogger(String n, MessageFactory f){throw new AssertionError();} }"#;
const LOGGER:&str=r#"package org.apache.logging.log4j; public interface Logger { String getName(); void info(String s,Object o); boolean isEnabled(Level l); Object getMessageFactory(); }"#;
const LEVEL:&str=r#"package org.apache.logging.log4j; public enum Level { INFO; }"#;
const MF:&str=r#"package org.apache.logging.log4j.message; public interface MessageFactory {}"#;
const PMF:&str=r#"package org.apache.logging.log4j.message; public final class ParameterizedMessageFactory implements MessageFactory { public static final ParameterizedMessageFactory INSTANCE=null; }"#;
const PROBE:&str=include_str!("fixtures/log4j_message_factory/demo/Probe.java");
const ORACLE:&str=include_str!("fixtures/log4j_message_factory/java25.txt");
#[test] fn rows_match_java25(){let f=Fixture::build("log4j-message-factory",&[("org/apache/logging/log4j/LogManager.java",LM),("org/apache/logging/log4j/Logger.java",LOGGER),("org/apache/logging/log4j/Level.java",LEVEL),("org/apache/logging/log4j/message/MessageFactory.java",MF),("org/apache/logging/log4j/message/ParameterizedMessageFactory.java",PMF),("demo/Probe.java",PROBE)]); let mut h=f.jar.seed_host(StdHost::new()); assert_eq!(f.text_on(&mut h,"rows"),ORACLE); assert_eq!(h.unknown_host_calls(), vec!["org/apache/logging/log4j/Logger#isEnabled(Lorg/apache/logging/log4j/Level;)Z".to_string(),"org/apache/logging/log4j/Logger#getMessageFactory()Ljava/lang/Object;".to_string()]);}
