package demo;
import java.nio.file.*;
import net.minecraft.nbt.NbtIo;
public class Probe {
 public static void main(String[] a) throws Exception { System.out.println(rows()); }
 public static String rows() throws Exception { return missing()+"|"+existing()+"|"+nullPath(); }
 public static String missing() throws Exception { return Boolean.toString(NbtIo.read(Path.of("game/nbt-probe/missing.dat")) == null); }
 public static String existing() throws Exception { Path p=Path.of("game/nbt-probe/existing.dat"); Files.write(p,new byte[]{0}); try { return NbtIo.read(p)==null?"null":"tag"; } catch (Throwable t) { return t.getClass().getName(); } }
 public static String nullPath() { try { NbtIo.read((Path)null); return "ok"; } catch (Throwable t) { return t.getClass().getName()+":"+t.getMessage(); } }
}
