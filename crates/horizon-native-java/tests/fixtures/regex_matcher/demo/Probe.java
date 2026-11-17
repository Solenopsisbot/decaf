package demo;
import java.util.regex.Pattern;
public final class Probe {
  public static String rows() {
    String[] xs={"abc_1","","a-b","a b","é","名","Ａ","á"};
    Pattern p=Pattern.compile("\\w+");
    StringBuilder b=new StringBuilder();
    for(String s:xs) b.append(s).append(" => ").append(p.matcher(s).matches()).append('\n');
    return b.toString();
  }
  public static void main(String[] args){System.out.print(rows());}
}
