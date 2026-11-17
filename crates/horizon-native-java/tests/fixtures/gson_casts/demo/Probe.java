package demo;
import com.google.gson.Gson; import java.util.*;
public final class Probe { public static String rows() { Object m=new Gson().fromJson("{\"a\":1}", Map.class); StringBuilder s=new StringBuilder();
 try { Map x=(Map)m; s.append("map=ok|"); } catch(Throwable t){s.append("map="+t+"|");}
 try { AbstractMap x=(AbstractMap)m; s.append("abstract=ok|"); } catch(Throwable t){s.append("abstract="+t.getClass().getName()+":"+t.getMessage()+"|");}
 try { java.io.Serializable x=(java.io.Serializable)m; s.append("serial=ok|"); } catch(Throwable t){s.append("serial="+t.getClass().getName()+":"+t.getMessage()+"|");}
 try { List x=(List)m; s.append("list=ok|"); } catch(Throwable t){s.append("list="+t.getClass().getName()+":"+t.getMessage()+"|");}
 try { HashMap x=(HashMap)m; s.append("hash=ok|"); } catch(Throwable t){s.append("hash="+t.getClass().getName()+":"+t.getMessage()+"|");}
 try { Map.class.cast(m); s.append("mapcast=ok|"); } catch(Throwable t){s.append("mapcast="+t.getClass().getName()+":"+t.getMessage()+"|");}
 try { List.class.cast(m); s.append("listcast=ok|"); } catch(Throwable t){s.append("listcast="+t.getClass().getName()+":"+t.getMessage()+"|");}
 s.append("instance="+(m instanceof List)).append("|class="+m.getClass().getName()); return s.toString(); } }
