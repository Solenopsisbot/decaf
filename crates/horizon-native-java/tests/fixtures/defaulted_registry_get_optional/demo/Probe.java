package demo;
import net.minecraft.server.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import java.util.Optional;
public final class Probe {
  public static String rows() {
    var pig = Identifier.parse("minecraft:pig");
    var nope = Identifier.parse("minecraft:nope");
    var a = BuiltInRegistries.ENTITY_TYPE.getOptional(pig);
    var b = BuiltInRegistries.ENTITY_TYPE.getOptional(nope);
    return a.isPresent()+"|"+b.isPresent()+"|"+(a.get()==BuiltInRegistries.ENTITY_TYPE.getValue(pig))+"|"+b.equals(Optional.empty());
  }
  public static void main(String[] args) { SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.out.println(rows()); }
}
