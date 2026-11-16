package demo;
import org.lwjgl.system.MathUtil;
import org.apache.logging.log4j.util.Strings;
import org.apache.commons.compress.utils.Lists;
public final class Probe {
 public static String bits(){ return MathUtil.mathIsPoT(0)+"|"+MathUtil.mathIsPoT(1)+"|"+MathUtil.mathIsPoT(Integer.MIN_VALUE)+"|"+MathUtil.mathIsPoT(-1)+"|"+MathUtil.mathRoundPoT(0)+"|"+MathUtil.mathRoundPoT(1)+"|"+MathUtil.mathRoundPoT(2)+"|"+MathUtil.mathRoundPoT(3)+"|"+MathUtil.mathRoundPoT(5)+"|"+MathUtil.mathRoundPoT(Integer.MAX_VALUE); }
 public static String strings(){ return Strings.isEmpty(null)+"|"+Strings.isEmpty("")+"|"+Strings.isEmpty("x")+"|"+Strings.isNotEmpty(null)+"|"+Strings.isNotEmpty("x")+"|"+Strings.isBlank(null)+"|"+Strings.isBlank(" \t")+"|"+Strings.isNotBlank(" "); }
 public static String whitespace(){ return Strings.isBlank("\u00a0")+"|"+Strings.isBlank("\u2007")+"|"+Strings.isBlank("\u202f")+"|"+Strings.isBlank("\u0085")+"|"+Strings.isBlank("\u001c")+"|"+Strings.isBlank("\u001f")+"|"+Strings.isBlank("\u2028")+"|"+Strings.isBlank("\u3000")+"|"+Strings.isBlank("\t\n\u000b\f\r")+"|"+Strings.isBlank("😀"); }
 public static String nonString(){
   int before=ProbeHost.unknowns();
   boolean empty=Strings.isEmpty(new StringBuilder());
   boolean jar; try { jar=Strings.isEmpty(new EmptyChars()); } catch (Throwable t) { jar=false; }
   return empty+"|"+jar+"|"+(ProbeHost.unknowns()-before);
 }
 public static String list(){ java.util.ArrayList<Object> x=Lists.newArrayList(); x.add("x"); return (x!=null)+"|"+x.size(); }
 public static final class EmptyChars implements CharSequence { public int length(){return 0;} public char charAt(int i){throw new IndexOutOfBoundsException();} public CharSequence subSequence(int a,int b){return this;} }
 public static final class ProbeHost { static int unknowns(){ return 0; } }
}
