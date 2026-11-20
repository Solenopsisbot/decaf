import net.minecraft.world.level.storage.loot.functions.LootItemConditionalFunction;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.storage.loot.LootContext;
import net.minecraft.world.level.storage.loot.predicates.LootItemCondition;
import net.minecraft.core.Holder;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import com.mojang.datafixers.Products;
import com.mojang.datafixers.kinds.App;
import com.mojang.serialization.MapCodec;
import com.mojang.serialization.JsonOps;
import java.lang.reflect.Field;
import java.util.Optional;
import java.util.function.Function;
import java.util.stream.Collectors;

public final class Probe extends LootItemConditionalFunction {
  Probe(Optional<Holder<LootItemCondition>> c) { super(c); }
  public com.mojang.serialization.MapCodec<? extends LootItemConditionalFunction> codec(){return null;}
  protected ItemStack run(ItemStack a,LootContext b){return a;}
  static LootItemCondition sentinel() {
    return new LootItemCondition() {
      public com.mojang.serialization.MapCodec<? extends LootItemCondition> codec(){return null;}
      public boolean test(LootContext c){return true;}
    };
  }
  static String shape() throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    Products.P1<?,?> p = commonFields(RecordCodecBuilder.instance());
    Object t1 = p.t1();
    Field gf = RecordCodecBuilder.class.getDeclaredField("getter");
    gf.setAccessible(true);
    @SuppressWarnings("unchecked") Function<Object,Object> getter = (Function<Object,Object>) gf.get(t1);
    Optional<Holder<LootItemCondition>> empty = Optional.empty();
    Optional<Holder<LootItemCondition>> nonempty = Optional.of(Holder.direct(sentinel()));
    boolean emptySame = getter.apply(new Probe(empty)) == empty;
    Object got = getter.apply(new Probe(nonempty));
    boolean nonemptySame = got == nonempty;
    MapCodec<?> codec = RecordCodecBuilder.build((App) t1);
    String keys = codec.keys(JsonOps.INSTANCE).map(Object::toString).sorted().collect(Collectors.joining(","));
    return p.getClass().getName() + "|" + t1.getClass().getName() + "|" +
      "emptySame=" + emptySame + "|nonemptySame=" + nonemptySame +
      "|codec=" + codec.getClass().getName() + "|keys=" + keys;
  }
  public static void main(String[] a) throws Exception { System.out.print(shape()); }
}
