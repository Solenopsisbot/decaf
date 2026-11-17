package demo;
import java.net.*;
public class Probe {
 public static String rows(){
  String[] x={"http://a.b/c?d#e","https://a.b/","file:///tmp/x","jar:file:/x.jar!/y","ftp://host/x","mailto:a@b","jrt:/java.base","jmod:/x","data:text/plain,x","nio:/x","a/b:c","//host/x","relative","HTTP://a/b","FILE:///T","JRT:/a","Jar:file:/x.jar!/y","jar:x","jar:file:///x.jar!/y","http://a.b","file:x","http:x","http://u@h:80/p","http://a/%7e?q#f","DATA:x","Nio:/x","Http://A.B/C"};StringBuilder b=new StringBuilder();for(String s:x)try{URL u=URI.create(s).toURL();b.append(s).append(" => ").append(u).append(" | ").append(u.toExternalForm()).append(" | ").append(u.getProtocol()).append('\n');}catch(Exception e){b.append(s).append(" => ").append(e.getClass().getName()).append(':').append(e.getMessage()).append('\n');}try{URI.create("http://a").toURL().openStream();}catch(Exception e){} b.append("unknown=openStream\n");return b.toString();} public static void main(String[]a){System.out.print(rows());}}
