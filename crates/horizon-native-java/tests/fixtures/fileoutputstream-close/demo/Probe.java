package demo;
import java.io.*; import java.nio.file.*;
public class Probe {
  interface X { void run() throws Exception; }
  static String err(X r){ try{r.run(); return "ok";}catch(Exception x){return x.getClass().getName()+":"+x.getMessage();} }
  public static String rows() {
    try {
      Path d=Path.of("fos-scratch"); Files.createDirectories(d); Path f=d.resolve("x.bin"); Files.deleteIfExists(f);
      FileOutputStream o=new FileOutputStream(f.toFile()); byte[] b={9,8,7,6}; o.write(b,1,2); o.write(b,0,0); o.close(); o.close();
      String bytes=java.util.Arrays.toString(Files.readAllBytes(f));
      String after=err(()->{o.write(b,0,1);});
      String negOff=err(()->{new FileOutputStream(d.resolve("n").toFile()).write(b,-1,1);});
      String negLen=err(()->{new FileOutputStream(d.resolve("m").toFile()).write(b,0,-1);});
      String over=err(()->{new FileOutputStream(d.resolve("q").toFile()).write(b,3,2);});
      String nul=err(()->{new FileOutputStream(d.resolve("z").toFile()).write(null,0,1);});
      Path nd=Path.of("missing-parent"); Files.deleteIfExists(nd.resolve("x.zip")); String parent=err(()->{new FileOutputStream(nd.resolve("x.zip").toFile());});
      return "bytes="+bytes+";after="+after+";negOff="+negOff+";negLen="+negLen+";over="+over+";null="+nul+";parent="+parent;
    } catch(Exception x){return "ERR:"+x;}
  }
  public static void main(String[] a){System.out.println(rows());}
}
