package oracle;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
public final class EntityTypesCapture {
 public static void main(String[] args) {
  SharedConstants.tryDetectVersion();
  Bootstrap.bootStrap();
  BuiltInRegistries.ENTITY_TYPE.keySet().forEach(System.out::println);
 }
}
