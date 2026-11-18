package demo;
import net.minecraft.SharedConstants;
import net.minecraft.WorldVersion;
public final class Probe {
 public static String buildTime() { return "" + SharedConstants.getCurrentVersion().buildTime(); }
 public static String rows() {
  WorldVersion v=SharedConstants.getCurrentVersion();
  StringBuilder o=new StringBuilder();
  o.append("name|").append(v.name()).append('\n');
  o.append("id|").append(v.id()).append('\n');
  o.append("protocol|").append(v.protocolVersion()).append('\n');
  o.append("stable|").append(v.stable()).append('\n');
  o.append("same|").append(SharedConstants.getCurrentVersion() == v).append('\n');
  o.append("detected-same|").append(SharedConstants.getCurrentVersion() == SharedConstants.getCurrentVersion()).append('\n');
  o.append("current-detected|").append(SharedConstants.getCurrentVersion() == v).append('\n');
  
  return o.toString();
 }
}
