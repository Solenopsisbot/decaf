package demo;
import java.nio.file.*; import java.nio.charset.*; import java.io.*; import java.util.*;
public final class Probe {
 static final byte[][] CASES={ {(byte)0xC0,(byte)0x80},{(byte)0xE0,(byte)0x80,(byte)0x80},{(byte)0xED,(byte)0xA0,(byte)0x80},{(byte)0xF5},{(byte)0xFF},{(byte)0x80},{(byte)0xE2},{(byte)0xF0,(byte)0x90}, "ok-✓".getBytes(StandardCharsets.UTF_8)};
 static String one(String n, int i){try{Path p=Path.of("/tmp/nu"+i); Files.write(p,CASES[i]); return switch(n){case "readString"->Files.readString(p);case "readAllLines"->Files.readAllLines(p).toString();case "lines"->Files.lines(p).toList().toString();case "buffered"->Files.newBufferedReader(p).readLine();case "fileReader"->new BufferedReader(new FileReader(p.toFile())).readLine();case "inputReader"->new BufferedReader(new InputStreamReader(Files.newInputStream(p),StandardCharsets.UTF_8)).readLine();case "string"->new String(Files.readAllBytes(p),StandardCharsets.UTF_8);case "bytes"->Arrays.toString(Files.readAllBytes(p));case "input"->Arrays.toString(Files.newInputStream(p).readAllBytes());default->"?";};}catch(Throwable e){return e.getClass().getName()+":"+e.getMessage();}}
 public static String rows(){StringBuilder b=new StringBuilder(); for(String n: List.of("readString","readAllLines","lines","buffered","fileReader","inputReader","string","bytes","input")){b.append(n).append('=');for(int i=0;i<CASES.length;i++){if(i>0)b.append(',');b.append(one(n,i));}b.append('\n');}return b.toString();}
 public static String counted() throws Exception{
  Path p=Path.of("/tmp/nu-counted");
  Files.write(p,new byte[]{(byte)0xC3,(byte)0x28});
  try{Files.readString(p,StandardCharsets.ISO_8859_1);}catch(Throwable ignored){}
  try{Files.readAllLines(p,StandardCharsets.ISO_8859_1);}catch(Throwable ignored){}
  try{Files.newBufferedReader(p,StandardCharsets.ISO_8859_1);}catch(Throwable ignored){}
  return "ok";
 }

}
