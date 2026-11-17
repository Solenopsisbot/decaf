import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.ai.attributes.AttributeModifier;
import net.minecraft.world.entity.HumanoidArm;
public final class VanillaEnumOracle {
  static void dump(Class<? extends Enum<?>> c) throws Exception {
    System.out.println("CLASS " + c.getName());
    for (Enum<?> e : c.getEnumConstants()) {
      String extra = e instanceof SoundSource s ? " getName=" + s.getName() : "";
      System.out.println(e.name()+"|"+e.ordinal()+"|"+e.toString()+extra);
    }
  }
  public static void main(String[] args) throws Exception {
    dump(SoundSource.class);
    dump(AttributeModifier.Operation.class);
    dump(HumanoidArm.class);
  }
}
