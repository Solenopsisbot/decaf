package demo;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.network.syncher.*;
import net.minecraft.world.entity.*;
/**
 * Each `SynchedEntityData.defineId` is a new `EntityDataAccessor` (#669): two unrelated holders'
 * first ids are both 0, and still two objects. Run against the real 26.3 server jar (`command.txt`).
 */
public class AccessorProbe {
 static final EntityDataSerializer<Integer> S = new EntityDataSerializer<Integer>() { public net.minecraft.network.codec.StreamCodec<? super net.minecraft.network.RegistryFriendlyByteBuf,Integer> codec(){return null;} public Integer copy(Integer x){return x;} };
 static class Base implements SyncedDataHolder { public void onSyncedDataUpdated(EntityDataAccessor<?> a) {} public void onSyncedDataUpdated(java.util.List<SynchedEntityData.DataValue<?>> a) {} }
 static class Sibling implements SyncedDataHolder { public void onSyncedDataUpdated(EntityDataAccessor<?> a) {} public void onSyncedDataUpdated(java.util.List<SynchedEntityData.DataValue<?>> a) {} }
 public static String rows() {
  EntityDataAccessor<Integer> base = SynchedEntityData.defineId(Base.class, S);
  EntityDataAccessor<Integer> sibling = SynchedEntityData.defineId(Sibling.class, S);
  EntityDataAccessor<Integer> base2 = SynchedEntityData.defineId(Base.class, S);
  return base.id() + "," + sibling.id() + "," + base2.id() + "|" + (base == sibling) + "," + (base == base2) + "," + (base == base);
 }
 public static void main(String[] x) { SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.out.println("ROW " + rows()); }
}
