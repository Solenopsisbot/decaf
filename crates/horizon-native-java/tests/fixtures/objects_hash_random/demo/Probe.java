package demo;
import java.util.Objects;
import java.util.Random;
public final class Probe {
 public static String objectsRows() {
  Object first=Integer.valueOf(7), second=Long.valueOf(9L), text="decaf", nothing=null;
  Object[] values=new Object[]{first,second,text,nothing};
  return Integer.toString(Objects.hash(values))+"|"+Integer.toString(Objects.hash(text,first,nothing));
 }
 public static String seededRandomRows() {
  Random random=new Random(123456789L);
  return Integer.toString(random.nextInt(1000))+"|"+Integer.toString(random.nextInt(31))+"|"+Integer.toString(random.nextInt());
 }
 public static String badBound() {
  try { return Integer.toString(new Random(5L).nextInt(0)); }
  catch (IllegalArgumentException e) { return "IllegalArgumentException:"+e.getMessage(); }
 }
 public static String rangedRandom() { return Integer.toString(new Random(1L).nextInt(0, 16)); }
 public static String unseededRandom() { Random random=new Random(); return Integer.toString(random.nextInt(10)); }
 public static String unknownObjectHash() { return Integer.toString(Objects.hash(new Object())); }
}
