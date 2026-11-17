import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.network.syncher.*;
import net.minecraft.world.entity.*;
public class Probe {
 static final EntityDataSerializer<Integer> S = new EntityDataSerializer<Integer>() { public net.minecraft.network.codec.StreamCodec<? super net.minecraft.network.RegistryFriendlyByteBuf,Integer> codec(){return null;} public Integer copy(Integer x){return x;} };
 static class Base implements SyncedDataHolder { public void onSyncedDataUpdated(EntityDataAccessor<?> a) {} public void onSyncedDataUpdated(java.util.List<SynchedEntityData.DataValue<?>> a) {} }
 static class Sub extends Base { }
 public static String rows() {
  StringBuilder out=new StringBuilder();
  for (MobCategory c : MobCategory.values()) out.append("cat="+c.name()+","+c.getName()+","+c.getDebugAbbreviation()+","+c.getMaxInstancesPerChunk()+","+c.isFriendly()+","+c.isPersistent()+","+c.getDespawnDistance()+","+c.getNoDespawnDistance()+","+c.toString()).append("\n");
  out.append("serial="+(EntityDataSerializers.BYTE==EntityDataSerializers.BYTE)+","+EntityDataSerializers.BYTE.equals(EntityDataSerializers.INT)).append("\n");
  out.append("base="+SynchedEntityData.defineId(Base.class,S).id()).append("\n");
  out.append("sub="+SynchedEntityData.defineId(Sub.class,S).id()).append("\n");
  out.append("base2="+SynchedEntityData.defineId(Base.class,S).id()).append("\n");
  class Sibling implements SyncedDataHolder { public void onSyncedDataUpdated(EntityDataAccessor<?> a) {} public void onSyncedDataUpdated(java.util.List<SynchedEntityData.DataValue<?>> a) {} }
  out.append("sibling="+SynchedEntityData.defineId(Sibling.class,S).id()).append("\n");
  try { for(int i=0;i<256;i++) SynchedEntityData.defineId(Overflow.class,S); SynchedEntityData.defineId(Overflow.class,S); } catch (IllegalArgumentException e) { out.append("overflow="+e.getMessage()).append("\n"); }
  return out.toString();
 }
 public static void main(String[] x) { SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.out.print(rows()); }
 static class Overflow implements SyncedDataHolder { public void onSyncedDataUpdated(EntityDataAccessor<?> a) {} public void onSyncedDataUpdated(java.util.List<SynchedEntityData.DataValue<?>> a) {} }
}
