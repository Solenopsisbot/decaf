package demo;

import com.mojang.datafixers.kinds.App;
import com.mojang.datafixers.util.Pair;
import com.mojang.datafixers.util.Either;
import com.mojang.serialization.Codec;
import com.mojang.serialization.Lifecycle;
import com.mojang.serialization.MapCodec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.resources.Identifier;
import java.util.*;

/** DataFixerUpper's Pair and the game's codecs as host objects (#156 P5c). */
public final class HostCodecs {
  static final class SubPair extends Pair<Integer, String> {
    int extra = 7;
    SubPair() { super(1, "sub"); }
    String own() { return "own" + extra; }
  }
  static final String[] OLD = {
    "dfu-pair/i:1,a",
    "nf-codec/0",
    "nf-codec/compound-tag",
    "com/mojang/serialization/Codec#codec",
    "codec/opaque",
  };

  static int identities(Object... objects) {
    Map<Object, Boolean> seen = new IdentityHashMap<>();
    for (Object o : objects) seen.put(o, true);
    return seen.size();
  }

  /** Strings spelled like the old markers are Strings, equal to no pair. */
  public static String spelled() {
    Object p = Pair.of(1, "a");
    StringBuilder out = new StringBuilder();
    for (Object t : OLD) {
      out.append(t.equals(p)).append(p.equals(t)).append("" + t).append(((String) t).length()).append(',');
    }
    return out.append(List.of(OLD).indexOf(OLD[0])).append(List.of(p).indexOf(OLD[0])).toString();
  }

  /** `word/rest` text is not yet told from a marker (#156 P17): these checks are counted. */
  public static String spelledTypes() {
    StringBuilder out = new StringBuilder();
    for (Object t : OLD) {
      out.append(t instanceof String).append(t instanceof Pair).append(t instanceof Codec).append(',');
    }
    return out.toString();
  }

  /** PR1 DFU construction: static singleton, fresh field/xmap nodes, and callback retention. */
  public static String dfuConstruction() {
    Codec<Integer> codec = Codec.INT;
    Object field = codec.fieldOf("n");
    Object getter = ((com.mojang.serialization.MapCodec<Integer>) field).forGetter((Integer x) -> x);
    Object mapped = codec.xmap(x -> x + 1, x -> x - 1);
    Object record = com.mojang.serialization.codecs.RecordCodecBuilder.create(i -> (App) null);
    return (codec == Codec.INT) + "|" + ((Object) codec == (Object) Codec.STRING) + "|" + (field instanceof com.mojang.serialization.MapCodec)
        + "|" + (field == codec.fieldOf("n")) + "|" + (getter instanceof com.mojang.serialization.codecs.RecordCodecBuilder)
        + "|" + (mapped instanceof Codec) + "|" + (mapped == codec.xmap(x -> x + 1, x -> x - 1))
        + "|" + (record instanceof Codec);
  }

  /** A pair hands back its values; equal by them, a new object each time; text and hash. */
  public static String pairs() {
    Pair<Integer, String> p = Pair.of(1, "a");
    Pair<Integer, String> q = Pair.of(1, "a");
    Pair<String, Integer> s = p.swap();
    Object o = p;
    return "" + p.getFirst() + p.getSecond() + s.getFirst() + s.getSecond() + "|" + (p == q) + p.equals(q)
        + p.equals(s) + s.swap().equals(p) + p.equals(Pair.of(1, "b")) + p.equals(null) + identities(p, p, q) + "|"
        + p + "|" + Pair.of(null, null) + "|" + p.hashCode() + "," + Pair.of(null, null).hashCode() + "|"
        + (o instanceof Pair) + (o instanceof App) + (o instanceof Integer) + o.getClass().getName()
        + o.getClass().getSuperclass().getName() + "|" + new HashSet<>(List.of(p)).contains(q)
        + new HashMap<>(Map.of(p, 7)).get(q) + List.of(s, q).indexOf(p);
  }

  /** The public constructor and map callbacks use the runtime callback path. */
  public static String pairMaps() {
    Pair<Integer, String> p = new Pair<>(2, "b");
    Pair<Integer, String> q = p.mapFirst(x -> x + 1).mapSecond(String::toUpperCase);
    return p + "|" + q + "|" + q.getFirst() + q.getSecond() + "|" + q.equals(Pair.of(3, "B"));
  }

  public static String pairSubclass() {
    SubPair p = new SubPair();
    return p.getClass().getName() + "|" + p.getFirst() + "|" + p.getSecond()
        + "|" + p.extra + "|" + p.own() + "|" + (p instanceof SubPair);
  }

  /** Either's selected branch maps, callbacks and Optional projections. */
  public static String eithers() {
    Either<String, Integer> left = Either.left("a");
    Either<String, Integer> right = Either.right(2);
    StringBuilder seen = new StringBuilder();
    left.ifLeft(seen::append).ifRight(x -> seen.append("bad"));
    right.ifLeft(x -> seen.append("bad")).ifRight(x -> seen.append(x + 1));
    return left + "|" + right + "|" + left.map(String::toUpperCase, Object::toString) + "|"
        + right.map(String::toUpperCase, Object::toString) + "|" + left.left().get() + "|"
        + right.right().get() + "|" + left.right().isPresent() + "|" + right.left().isPresent()
        + "|" + seen + "|" + left.swap();
  }

  public static String eitherIdentity() {
    Either<String, Integer> left = Either.left("a");
    Either<String, Integer> right = Either.right(2);
    return ((Object) left.mapRight(Object::toString) == left) + "|"
        + ((Object) right.mapLeft(String::toUpperCase) == right);
  }

  public static String eitherOrThrow() {
    try { return String.valueOf(Either.right(2).orThrow()); }
    catch (RuntimeException e) { return e.getClass().getName() + "|" + e.getMessage(); }
  }

  /** Nested pairs compare and print through their values. */
  public static String nested() {
    Pair<Pair<Integer, String>, List<Integer>> n = Pair.of(Pair.of(2, "b"), List.of(3, 4));
    return "" + n + "|" + n.hashCode() + "|" + n.equals(Pair.of(Pair.of(2, "b"), List.of(3, 4)));
  }

  /** A pair of an identity-hashed object: its hash and text are counted. */
  public static String identityHeld() {
    return "" + Pair.of(new Object(), 1).hashCode();
  }

  /** A library's codec constants are one object each (#199 F2). */
  public static String codecConstants() {
    Object s = Codec.STRING;
    Set<Object> set = new HashSet<>();
    return "" + (s == Codec.STRING) + (s == Codec.INT) + (ByteBufCodecs.INT == ByteBufCodecs.INT)
        + (ByteBufCodecs.INT == ByteBufCodecs.VAR_INT) + s.equals(Codec.STRING) + s.equals(Codec.INT)
        + identities(s, Codec.STRING, Codec.INT) + List.of(s).contains(Codec.STRING)
        + new ArrayList<>(List.of(Codec.INT, s)).indexOf(Codec.STRING) + set.add(Codec.BOOL) + set.add(Codec.BOOL)
        + set.size() + set.contains(Codec.BOOL);
  }

  /** `Set.of` refuses one constant twice, printing it (DataFixerUpper's `toString`). */
  /** Every DataFixerUpper Codec constant is one canonical object. */
  public static String codecSingletons() {
    return "" + (Codec.BOOL == Codec.BOOL) + (Codec.BYTE == Codec.BYTE) + (Codec.SHORT == Codec.SHORT)
        + (Codec.INT == Codec.INT) + (Codec.LONG == Codec.LONG) + (Codec.FLOAT == Codec.FLOAT)
        + (Codec.DOUBLE == Codec.DOUBLE) + (Codec.STRING == Codec.STRING) + (Codec.BYTE_BUFFER == Codec.BYTE_BUFFER)
        + (Codec.INT_STREAM == Codec.INT_STREAM) + (Codec.LONG_STREAM == Codec.LONG_STREAM)
        + (Codec.PASSTHROUGH == Codec.PASSTHROUGH) + (Codec.EMPTY == Codec.EMPTY)
        + ((Object) Codec.INT == (Object) Codec.LONG);
  }

  public static String codecDuplicate() {
    try {
      return "" + Set.of(Codec.STRING, Codec.STRING).size();
    } catch (IllegalArgumentException x) {
      return x.getMessage();
    }
  }

  public static String codecText() {
    return Codec.STRING + "|" + Codec.INT + "|" + Codec.BOOL;
  }

  record One(int a) {}
  record Three(int a, String b, boolean c) {}
  record Eight(int a, int b, int c, int d, int e, int f, int g, int h) {}
  static int throwingGetter(One value) {
    throw new IllegalStateException("getter");
  }

  /** Record builders execute their jar callback and retain fields/getters/constructors. */
  public static String recordConstruction() {
    Codec<One> one = com.mojang.serialization.codecs.RecordCodecBuilder.create(i ->
        i.group(Codec.INT.fieldOf("a").forGetter(One::a)).apply(i, One::new));
    MapCodec<Three> three = com.mojang.serialization.codecs.RecordCodecBuilder.mapCodec(i ->
        i.group(Codec.INT.fieldOf("a").forGetter(Three::a),
            Codec.STRING.optionalFieldOf("b", "").forGetter(Three::b),
            Codec.BOOL.optionalFieldOf("c", true).forGetter(Three::c))
            .apply(i, Three::new));
    Codec<Eight> eight = com.mojang.serialization.codecs.RecordCodecBuilder.create(i ->
        i.group(Codec.INT.fieldOf("a").forGetter(Eight::a),
            Codec.INT.fieldOf("b").forGetter(Eight::b),
            Codec.INT.fieldOf("c").forGetter(Eight::c),
            Codec.INT.fieldOf("d").forGetter(Eight::d),
            Codec.INT.fieldOf("e").forGetter(Eight::e),
            Codec.INT.fieldOf("f").forGetter(Eight::f),
            Codec.INT.fieldOf("g").forGetter(Eight::g),
            Codec.INT.fieldOf("h").forGetter(Eight::h))
            .apply(i, Eight::new));
    MapCodec<Integer> required = Codec.INT.fieldOf("required");
    MapCodec<?> optional = Codec.INT.optionalFieldOf("optional");
    MapCodec<Integer> defaulted = Codec.INT.optionalFieldOf("defaulted", 7);
    Codec<?> list = Codec.INT.listOf();
    Codec<Integer> mapped = Codec.INT.xmap(x -> x + 1, x -> x - 1);
    Codec<One> nested = RecordCodecBuilder.create(i ->
        i.group(one.fieldOf("one").forGetter((One x) -> x)).apply(i, x -> x));
    Codec<One> throwing = RecordCodecBuilder.create(i ->
        i.group(Codec.INT.fieldOf("a").forGetter(HostCodecs::throwingGetter)).apply(i, One::new));
    return (one instanceof Codec) + "|" + (three instanceof MapCodec) + "|" + (eight instanceof Codec)
        + "|" + (nested instanceof Codec) + "|" + (throwing instanceof Codec)
        + "|" + required + "|" + optional + "|" + defaulted + "|" + list + "|" + mapped
        + "|" + (Lifecycle.stable() == Lifecycle.stable()) + "|" + (Lifecycle.experimental() == Lifecycle.experimental())
        + "|" + Lifecycle.stable() + "|" + Lifecycle.experimental();
  }

  public static String identifierCodec() {
    Object first = Identifier.CODEC;
    Object second = Identifier.CODEC;
    return (first instanceof Codec) + "|" + (first == second);
  }

  /** A pair is never equal to an object that is not a pair (#199 F4). */
  public static String pairVsJdk() {
    Pair<Integer, String> p = Pair.of(1, "a");
    return "" + p.equals(new Object()) + p.equals(new ArrayList<>()) + p.equals(new AbstractMap.SimpleEntry<>(1, "a"))
        + p.equals(Map.entry(1, "a")) + p.equals(List.of(1, "a"));
  }

  /** An unmodelled codec constant: counted (Java: `truetrue`). */
  public static String opaqueCodec() {
    Object c = Codec.STRING;
    return "" + (c == Codec.STRING) + (c instanceof Codec);
  }
  /** Broad DFU operations are deliberately counted until the execution phase is modelled. */
  public static String countedOperations() {
    int calls = 0;
    try { Codec.INT.listOf(1, 3); } catch (Throwable ignored) {} finally { calls++; }
    try { ((Codec) Codec.INT).dispatch(null, null); } catch (Throwable ignored) {} finally { calls++; }
    try { ((Codec) Codec.INT).orElse((Codec) Codec.INT); } catch (Throwable ignored) {} finally { calls++; }
    try { Codec.INT.validate(x -> null); } catch (Throwable ignored) {} finally { calls++; }
    try { Codec.EMPTY.codec(); } catch (Throwable ignored) {} finally { calls++; }
    try { Codec.INT.encodeStart(null, 1); } catch (Throwable ignored) {} finally { calls++; }
    try { Codec.INT.decode(null, null); } catch (Throwable ignored) {} finally { calls++; }
    try { Codec.INT.parse(null, null); } catch (Throwable ignored) {} finally { calls++; }
    return Integer.toString(calls);
  }

  /** MapCodec.unit is fresh per call, while its structural text is deterministic. */
  public static String mapUnitIdentity() {
    MapCodec<String> a = MapCodec.unit("alpha");
    MapCodec<String> b = MapCodec.unit("alpha");
    return Boolean.toString(a == b);
  }

  public static String mapUnitText() {
    MapCodec<String> a = MapCodec.unit("alpha");
    return a + "|" + a.codec();
  }

  public static String mapUnitCodecIdentity() {
    MapCodec<String> a = MapCodec.unit("alpha");
    Object first = a.codec();
    Object second = a.codec();
    return Boolean.toString(first == second);
  }

  /** MapCodec.unit's empty keys, prefix-preserving encode, supplier-per-decode, and text. */
  public static String mapUnitExecution() {
    final int[] calls = {0};
    java.util.function.Supplier<String> supplier = () -> "sup" + (++calls[0]);
    MapCodec<String> map = MapCodec.unit(supplier);
    long keys = map.keys(null).count();
    String first = map.decode(null, null).result().orElseThrow();
    String second = map.decode(null, null).result().orElseThrow();
    com.mojang.serialization.RecordBuilder<Object> prefix = null;
    boolean samePrefix = map.encode("ignored", null, prefix) == prefix;
    String mapText = map.toString();
    String codecText = map.codec().toString();
    String fieldText = map.codec().fieldOf("x").toString();
    return first + "|" + second + "|" + calls[0] + "|" + keys + "|" + samePrefix + "|"
        + mapText + "|" + codecText + "|" + fieldText;
  }

  public static String mapUnitNull() {
    MapCodec<String> map = MapCodec.unit((String) null);
    com.mojang.serialization.RecordBuilder<Object> prefix = null;
    String decoded;
    try { decoded = Boolean.toString(map.decode(null, null).result().isPresent()); }
    catch (Throwable x) { decoded = x.getClass().getName() + "|" + x.getMessage(); }
    return map + "|" + decoded + "|" + (map.encode(null, null, prefix) == prefix);
  }

  /** StreamCodec.unit retains the value and enforces it on encode. */
  public static String streamUnitIdentity() {
    StreamCodec<Object, String> a = StreamCodec.unit("alpha");
    StreamCodec<Object, String> b = StreamCodec.unit("alpha");
    return Boolean.toString(a == b);
  }

  public static String streamUnitRoundTrip() {
    StreamCodec<Object, String> a = StreamCodec.unit("alpha");
    String decoded = a.decode(null);
    boolean accepted;
    try { a.encode(null, "alpha"); accepted = true; } catch (Throwable x) { accepted = false; }
    String rejected;
    try { a.encode(null, "beta"); rejected = "none"; }
    catch (Throwable x) { rejected = x.getClass().getName() + "|" + x.getMessage(); }
    return decoded + "|" + accepted + "|" + rejected;
  }

  public static String streamUnitNull() {
    StreamCodec<Object, String> codec = StreamCodec.unit(null);
    String decoded = String.valueOf(codec.decode(null));
    try {
      codec.encode(null, null);
      return decoded + "|accepted";
    } catch (Throwable x) {
      return decoded + "|" + x.getClass().getName() + "|" + x.getMessage();
    }
  }

  static final class Eq {
    final int value;
    Eq(int value) { this.value = value; }
    @Override public boolean equals(Object other) { return other instanceof Eq x && x.value == value; }
    @Override public String toString() { return "Eq" + value; }
  }

  public static String streamUnitCustomEquals() {
    StreamCodec<Object, Eq> codec = StreamCodec.unit(new Eq(7));
    boolean equalAccepted;
    try { codec.encode(null, new Eq(7)); equalAccepted = true; } catch (Throwable x) { equalAccepted = false; }
    boolean unequalRejected;
    try { codec.encode(null, new Eq(8)); unequalRejected = false; }
    catch (IllegalStateException x) { unequalRejected = true; }
    return codec.decode(null).toString() + "|" + equalAccepted + "|" + unequalRejected;
  }

  /** A network StreamCodec is not a DFU Codec and must be refused before dispatch. */
  public static String forgedDfuReceiver() {
    Object forged = StreamCodec.unit(1);
    if (!(forged instanceof Codec)) return "true";
    try {
      ((Codec<Object>) forged).xmap(x -> x, x -> x);
      return "false";
    } catch (Throwable expected) {
      return "true";
    }
  }
  /** Deterministic structural text for the phase-1 nodes. */
  public static String dfuText() {
    return Codec.INT.fieldOf("x") + "|" + Codec.INT.optionalFieldOf("x") + "|"
        + Codec.INT.optionalFieldOf("x", 7) + "|"
        + Codec.INT.optionalFieldOf("x", 7, (Lifecycle) null) + "|"
        + Codec.INT.xmap(x -> x + 1, x -> x - 1) + "|" + Codec.INT.listOf(1, 3);
  }
  /** A real game StreamCodec constant is rejected by every DFU receiver family. */
  public static String forgedDfuReceivers() {
    Object stream = StreamCodec.unit(1);
    Object game = StreamCodec.unit(2);
    return "" + rejectedCodec(stream) + rejectedCodec(game) + rejectedOptional(stream) + rejectedOptional(game)
        + rejectedXmap(stream) + rejectedXmap(game) + rejectedGetter(stream) + rejectedGetter(game);
  }

  static boolean rejectedCodec(Object value) {
    if (!(value instanceof Codec)) return true;
    try { ((Codec<Object>) value).fieldOf("x"); return false; } catch (Throwable expected) { return true; }
  }
  static boolean rejectedOptional(Object value) {
    if (!(value instanceof Codec)) return true;
    try { ((Codec<Object>) value).optionalFieldOf("x", 1); return false; } catch (Throwable expected) { return true; }
  }
  static boolean rejectedXmap(Object value) {
    if (!(value instanceof Codec)) return true;
    try { ((Codec<Object>) value).xmap(x -> x, x -> x); return false; } catch (Throwable expected) { return true; }
  }
  static boolean rejectedGetter(Object value) {
    if (!(value instanceof com.mojang.serialization.MapCodec)) return true;
    try { ((com.mojang.serialization.MapCodec<Object>) value).forGetter(x -> x); return false; } catch (Throwable expected) { return true; }
  }

  /** Builds xmap inside a callback, drops the callback/source references, forces GC, then uses the handle. */
  public static String callbackXmapUse() {
    final Codec<?>[] box = new Codec<?>[1];
    java.util.function.Supplier<Codec<?>> maker = () -> {
      Codec<?> value = Codec.INT.xmap(x -> x, x -> x);
      box[0] = value;
      return value;
    };
    Codec<?> kept = maker.get();
    maker = null;
    box[0] = null;
    for (int i = 0; i < 300; i++) {
      Object[] waste = new Object[1024];
      waste[0] = i;
    }
    System.gc();
    return kept.toString();
  }
  // One probe per text form, so each is either exact or counted on its own (#410 review).
  private static com.mojang.serialization.DataResult<Integer> ok(int value) {
    return com.mojang.serialization.DataResult.success(value);
  }

  /** `Codec.flatXmap`: the receiver's text and a suffix. */
  public static String textFlatXmap() {
    return String.valueOf(Codec.INT.flatXmap(x -> ok(x + 1), x -> ok(x - 1)));
  }

  /** `Codec.comapFlatMap`. */
  public static String textComapFlatMap() {
    return String.valueOf(Codec.INT.comapFlatMap(x -> ok(x + 1), x -> x - 1));
  }

  /** `MapCodec.xmap` over a field. */
  public static String textMapXmap() {
    return String.valueOf(Codec.INT.fieldOf("x").xmap(x -> x + 1, x -> x - 1));
  }

  /** `MapCodec.flatXmap` over a field. */
  public static String textMapFlatXmap() {
    return String.valueOf(Codec.INT.fieldOf("x").flatXmap(x -> ok(x + 1), x -> ok(x - 1)));
  }

  /** A list codec, and one mapped. */
  public static String textList() {
    return Codec.INT.listOf() + "|" + Codec.STRING.listOf().xmap(x -> x, x -> x);
  }

  /**
   * A record codec built by `create`, its map codec twin, and that one's `codec()`. Their text
   * holds a lambda's identity hash, which differs every run, so only its fixed start is read.
   */
  public static String textRecord() {
    Codec<One> created = RecordCodecBuilder.create(i ->
        i.group(Codec.INT.fieldOf("a").forGetter(One::a)).apply(i, One::new));
    MapCodec<One> mapped = RecordCodecBuilder.mapCodec(i ->
        i.group(Codec.INT.fieldOf("a").forGetter(One::a)).apply(i, One::new));
    String start = "RecordCodec[UnitDecoder[";
    return created.toString().startsWith(start) + "|" + mapped.toString().startsWith(start)
        + "|" + mapped.codec().toString().startsWith(start);
  }

  static int createCalls;

  /** `create` runs its builder callback once, while it is called. */
  public static String createCounter() {
    createCalls = 0;
    Codec<One> codec = RecordCodecBuilder.create(i -> {
      createCalls++;
      return i.group(Codec.INT.fieldOf("a").forGetter(One::a)).apply(i, One::new);
    });
    return createCalls + "|" + (codec != null);
  }

  /** Once too when the callback hands back nothing a builder made. */
  public static String createCounterNull() {
    createCalls = 0;
    RecordCodecBuilder.create(x -> { createCalls++; return (App) null; });
    return Integer.toString(createCalls);
  }

  public static String createThrows() { try { RecordCodecBuilder.create(x -> { throw new IllegalArgumentException("boom"); }); return "none"; } catch (Throwable t) { return t.getClass().getName() + ":" + t.getMessage(); } }
  public static String encodeUnknown() { try { Codec.INT.encodeStart(null, 1); } catch (Throwable ignored) {} return "called"; }
  public static String parseUnknown() { try { Codec.INT.parse(null, 1); } catch (Throwable ignored) {} return "called"; }

}
