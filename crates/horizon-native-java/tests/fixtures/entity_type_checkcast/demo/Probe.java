package demo;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.EntityType;
public final class Probe {
 static Identifier id(String s){ return Identifier.parse(s); }
 public static String rows(){ Object value=BuiltInRegistries.ENTITY_TYPE.getValue(id("minecraft:pig")); EntityType<?> cast=(EntityType<?>) value; return String.join("|", Boolean.toString(cast != null), Boolean.toString(value instanceof EntityType), Boolean.toString(value instanceof Object), Boolean.toString(BuiltInRegistries.BLOCK instanceof EntityType), Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getValue(id("minecraft:zombie")) instanceof EntityType)); }
 public static String undecidable(){ return Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getValue(id("minecraft:pig")) instanceof UnknownInterface); }
 public static String othermod(){ return Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getValue(id("othermod:thing")) instanceof EntityType); }
}
