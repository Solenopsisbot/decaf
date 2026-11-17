package demo;
import java.io.*; import java.nio.file.*;
public class Probe {
  public static String rows() {
    try {
      Path d=Path.of("fos-scratch"); Files.createDirectories(d); Path f=d.resolve("x.bin"); Files.deleteIfExists(f);
      FileOutputStream o=new FileOutputStream(f.toFile()); o.write(new byte[]{1,2,3}); o.close(); o.close();
      String a=java.util.Arrays.toString(Files.readAllBytes(f)); long n=Files.size(f);
      String e=""; try {o.write(4);} catch(Exception x){e=x.getClass().getName()+":"+x.getMessage();}
      FileOutputStream ap=new FileOutputStream(f.toFile(),true); ap.write(new byte[]{4,5}); ap.close();
      return "read="+a+";size="+n+";after="+e+";append="+java.util.Arrays.toString(Files.readAllBytes(f));
    } catch(Exception x){return "ERR:"+x;}
  }
  public static void main(String[] a){System.out.println(rows());}
}
