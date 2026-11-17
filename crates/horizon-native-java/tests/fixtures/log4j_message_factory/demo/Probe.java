package demo;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.Level;
import org.apache.logging.log4j.message.MessageFactory;
import org.apache.logging.log4j.message.ParameterizedMessageFactory;
public final class Probe {
  static final MessageFactory F = ParameterizedMessageFactory.INSTANCE;
  public static void main(String[] x){System.out.print(rows());}
  public static String rows() {
    Logger a = LogManager.getLogger("probe", F);
    Logger b = LogManager.getLogger("probe", F);
    Logger plain = LogManager.getLogger("probe");
    Logger other = LogManager.getLogger("other", F);
    a.info("x {}", 1);
    return a.getName()+"|"+(a==b)+"|"+(plain==a)+"|"+(other==a);
  }
  public static String counted() { Logger a=LogManager.getLogger("probe",F); a.isEnabled(Level.INFO); a.getMessageFactory(); return "ok"; }
}
