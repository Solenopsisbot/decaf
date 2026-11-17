package demo;
import java.lang.reflect.Array;
public class Probe {
 public static String getLength(){try{return String.valueOf(Array.getLength("[I#0"));}catch(Throwable t){return t.getClass().getName()+":"+t.getMessage();}}
 public static String copySrc(){try{System.arraycopy("[I#0",0,new int[1],0,1);return "ok";}catch(Throwable t){return t.getClass().getName()+":"+t.getMessage();}}
 public static String copyDst(){try{System.arraycopy(new int[]{42},0,"[I#0",0,1);return "ok";}catch(Throwable t){return t.getClass().getName()+":"+t.getMessage();}}
}
