import com.mojang.serialization.Codec;
import com.mojang.serialization.DataResult;
import java.util.TreeSet;
import net.minecraft.nbt.ByteTag;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.DoubleTag;
import net.minecraft.nbt.FloatTag;
import net.minecraft.nbt.IntArrayTag;
import net.minecraft.nbt.IntTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.LongTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.ShortTag;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;

/**
 * Captures {@code codec.encodeStart(NbtOps.INSTANCE, value)} on Minecraft 26.3 and OpenJDK 25 for
 * each of {@code demo.Cases}: one line {@code CASE:<name>=<tag>} per case, the tag printed with its
 * type ({@code int(4)}, {@code list[...]}, {@code compound{key:...}} with sorted keys, floats and
 * doubles as their bits) so the fixture compares types as well as values, or
 * {@code ERROR|<message>} for a codec error.
 */
public final class NbtOracle {
  static String show(Tag tag) {
    if (tag instanceof ByteTag b) return "byte(" + b.value() + ")";
    if (tag instanceof ShortTag s) return "short(" + s.value() + ")";
    if (tag instanceof IntTag i) return "int(" + i.value() + ")";
    if (tag instanceof LongTag l) return "long(" + l.value() + ")";
    if (tag instanceof FloatTag f) return String.format("float(%08x)", Float.floatToRawIntBits(f.value()));
    if (tag instanceof DoubleTag d) return String.format("double(%016x)", Double.doubleToRawLongBits(d.value()));
    if (tag instanceof StringTag s) return "string(" + s.value() + ")";
    if (tag instanceof IntArrayTag a) return "int_array" + java.util.Arrays.toString(a.getAsIntArray());
    if (tag instanceof ListTag list) {
      StringBuilder out = new StringBuilder("list[");
      for (int i = 0; i < list.size(); i++) out.append(i == 0 ? "" : ",").append(show(list.get(i)));
      return out.append("]").toString();
    }
    if (tag instanceof CompoundTag compound) {
      StringBuilder out = new StringBuilder("compound{");
      boolean first = true;
      for (String key : new TreeSet<>(compound.keySet())) {
        out.append(first ? "" : ",").append(key).append(":").append(show(compound.get(key)));
        first = false;
      }
      return out.append("}").toString();
    }
    return "other(" + tag.getClass().getName() + ")";
  }

  @SuppressWarnings({"rawtypes", "unchecked"})
  public static void main(String[] args) {
    for (int n = 0; n < demo.Cases.COUNT; n++) {
      Codec codec = demo.Cases.codec(n);
      DataResult<Tag> result = codec.encodeStart(NbtOps.INSTANCE, demo.Cases.value(n));
      String shown =
          result
              .result()
              .map(NbtOracle::show)
              .orElseGet(() -> "ERROR|" + result.error().get().message());
      System.out.println("CASE:" + demo.Cases.name(n) + "=" + shown);
    }
  }
}
