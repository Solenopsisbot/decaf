package demo;
import it.unimi.dsi.fastutil.objects.*;
public class Probe { public static String rows(){ Object2BooleanLinkedOpenHashMap<String> m=new Object2BooleanLinkedOpenHashMap<>(); for(int i=0;i<12;i++)m.put("k"+i,(i&1)==0); m.put("k3",true); m.removeBoolean("k4"); m.put("k4",false); StringBuilder s=new StringBuilder(); for(Object2BooleanMap.Entry<String> e:m.object2BooleanEntrySet()) s.append(e.getKey()).append('=').append(e.getBooleanValue()).append(';'); return s.toString(); } public static void main(String[]a){System.out.print(rows());}}
