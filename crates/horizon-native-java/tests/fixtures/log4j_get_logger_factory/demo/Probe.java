package demo;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.AbstractMessageFactory;
import org.apache.logging.log4j.message.FormattedMessageFactory;
import org.apache.logging.log4j.message.Message;
import org.apache.logging.log4j.message.MessageFactory;
import org.apache.logging.log4j.message.ObjectMessage;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.logging.log4j.message.SimpleMessage;

/** `LogManager.getLogger(String, MessageFactory)` with a mod's own factory (#483 family). */
public class Probe {
    /** Like the `PrefixingMessageFactory`s mods ship. */
    static final class A extends AbstractMessageFactory {
        public Message newMessage(String s) { return new SimpleMessage(s); }
        public Message newMessage(String s, Object... a) { return new ParameterizedMessage(s, a); }
    }

    /** Like PingWheel's anonymous factory. */
    static final class B extends FormattedMessageFactory {
        public Message newMessage(String s) { return new SimpleMessage(s); }
    }

    /** A plain `MessageFactory`: Log4j wraps it, so `getMessageFactory()` is another object. */
    static final class C implements MessageFactory {
        public Message newMessage(Object o) { return new ObjectMessage(o); }
        public Message newMessage(String s) { return new SimpleMessage(s); }
        public Message newMessage(String s, Object... a) { return new ParameterizedMessage(s, a); }
    }

    static String row(MessageFactory f1, MessageFactory f2) {
        Logger a = LogManager.getLogger("x", f1);
        Logger b = LogManager.getLogger("x", f1);
        Logger c = LogManager.getLogger("x", f2);
        // Logging through a factory logger is any logger's output call (Decaf: a no-op).
        a.info("hello {}", "x");
        return (a.getMessageFactory() == f1) + ":" + (a == b) + ":" + (a != c) + ":" + a.getName();
    }

    public static String rows() {
        A a1 = new A(), a2 = new A();
        B b1 = new B(), b2 = new B();
        // A null factory is the default one: the same logger as getLogger(name).
        Logger n = LogManager.getLogger("n", (MessageFactory) null);
        return row(a1, a2) + "|" + row(b1, b2) + "|" + (n == LogManager.getLogger("n")) + ":" + n.getName();
    }

    /** Counted in Decaf: the wrapper Log4j makes is not an object the host holds. */
    public static String plainFactory() {
        C c = new C();
        return "" + (LogManager.getLogger("p", c).getMessageFactory() == c);
    }

    /** Counted in Decaf: a level check needs a logging configuration the host doesn't have. */
    public static String levelCheck() {
        return "" + LogManager.getLogger("lc", new A()).isDebugEnabled();
    }

    public static void main(String[] x) {
        System.out.print(rows());
        System.out.print("\nplainFactory=" + plainFactory());
        System.out.print("\nlevelCheck=" + levelCheck());
    }
}
