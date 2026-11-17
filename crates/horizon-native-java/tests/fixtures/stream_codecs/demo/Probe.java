package demo;

import java.util.ArrayList;
import java.util.HashMap;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;

/** ByteBufCodecs.map / collection, built at startup as mods do (#472). */
@SuppressWarnings({"rawtypes", "unchecked"})
public final class Probe {
  /** Construction only: exact, nothing counted. */
  public static String rows() {
    StreamCodec<?, ?> m1 = ByteBufCodecs.map(HashMap::new, ByteBufCodecs.VAR_INT, ByteBufCodecs.STRING_UTF8);
    StreamCodec<?, ?> m2 = ByteBufCodecs.map(HashMap::new, ByteBufCodecs.VAR_INT, ByteBufCodecs.STRING_UTF8);
    StreamCodec<?, ?> c1 = ByteBufCodecs.collection(ArrayList::new, ByteBufCodecs.VAR_INT);
    StreamCodec<?, ?> c2 = ByteBufCodecs.collection(ArrayList::new, ByteBufCodecs.VAR_INT);
    StringBuilder s = new StringBuilder();
    s.append("distinctMaps=").append(m1 != m2).append('\n');
    s.append("distinctCollections=").append(c1 != c2).append('\n');
    s.append("mapIsCodec=").append(m1 instanceof StreamCodec).append('\n');
    s.append("collectionIsCodec=").append(c1 instanceof StreamCodec).append('\n');
    // Vanilla builds the codec lazily: a null factory doesn't throw until it's used.
    String nullFactory;
    try {
      ByteBufCodecs.map(null, ByteBufCodecs.VAR_INT, ByteBufCodecs.STRING_UTF8);
      nullFactory = "ok";
    } catch (RuntimeException e) {
      nullFactory = e.getClass().getName();
    }
    s.append("nullFactory=").append(nullFactory).append('\n');
    return s.toString();
  }

  /** Using a codec isn't modelled: counted. (Java throws a NullPointerException on these nulls.) */
  public static String counted() {
    StreamCodec m = ByteBufCodecs.map(HashMap::new, ByteBufCodecs.VAR_INT, ByteBufCodecs.STRING_UTF8);
    try {
      m.encode(null, null);
    } catch (RuntimeException e) {
      return e.getClass().getName();
    }
    return "returned";
  }

  public static void main(String[] a) {
    System.out.print(rows());
  }
}
