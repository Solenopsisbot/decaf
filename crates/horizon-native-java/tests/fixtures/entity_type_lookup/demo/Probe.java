package demo;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
public final class Probe {
 static Identifier id(String s){ return Identifier.parse(s); }
 public static String rows(){ return String.join("|", Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getOptional(id("minecraft:pig")).isPresent()), Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getOptional(id("minecraft:nope")).isPresent()), Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getOptional(id("minecraft:pig")).get()==BuiltInRegistries.ENTITY_TYPE.getOptional(id("minecraft:pig")).get()), Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getOptional(id("minecraft:zombie")).isPresent()), Boolean.toString(BuiltInRegistries.ENTITY_TYPE.get(id("minecraft:pig")).isPresent()), Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getValue(id("minecraft:pig"))==BuiltInRegistries.ENTITY_TYPE.getOptional(id("minecraft:pig")).get()), Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getValue(id("minecraft:nope"))==BuiltInRegistries.ENTITY_TYPE.getOptional(id("minecraft:pig")).get())); }
 public static String block(){ return Boolean.toString(BuiltInRegistries.BLOCK.containsKey(id("minecraft:stone"))); }
 public static String othermod(){ return Boolean.toString(BuiltInRegistries.ENTITY_TYPE.getOptional(id("othermod:thing")).isPresent()); }
}
