package demo;
import java.nio.file.*;
public final class Probe {
 static String q(java.util.function.Supplier<Object> s){try{return String.valueOf(s.get());}catch(RuntimeException e){return e.getClass().getSimpleName()+":"+e.getMessage();}}
 static String row(String n, java.util.function.Supplier<Object> s){return n+"="+q(s)+"\n";}
 public static String rows(){
  Path abs=Path.of("/a/b/../c"), rel=Path.of("a/b"), one=Path.of("a"), root=Path.of("/"), empty=Path.of(""), trail=Path.of("/a/b/"), repeated=Path.of("//a//b");
  StringBuilder x=new StringBuilder();
  x.append(row("abs.file",abs::getFileName)).append(row("abs.parent",abs::getParent)).append(row("abs.root",abs::getRoot)).append(row("abs.count",abs::getNameCount));
  x.append(row("rel.parent",rel::getParent)).append(row("rel.root",rel::getRoot)).append(row("one.parent",one::getParent)).append(row("one.root",one::getRoot));
  x.append(row("root.file",root::getFileName)).append(row("root.count",root::getNameCount)).append(row("empty.file",empty::getFileName));
  x.append(row("trail.file",trail::getFileName)).append(row("repeated",repeated::toString));
  x.append(row("name.neg",()->abs.getName(-1))).append(row("name.count",()->abs.getName(abs.getNameCount()))).append(row("sub.bad",()->rel.subpath(1,1))).append(row("sub.all",()->rel.subpath(0,rel.getNameCount())));
  Path joined=Path.of("a", "b"); x.append(row("eq",()->rel.equals(joined))).append(row("hash",()->rel.hashCode()==joined.hashCode()));
  return x.toString();
 }
 public static void main(String[] x){System.out.print(rows());}
}
