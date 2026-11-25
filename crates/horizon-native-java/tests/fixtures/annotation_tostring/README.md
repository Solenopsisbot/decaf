# Annotation.toString, hashCode and equals

`demo/Probe.java` is the only class the test ships; it needs nothing but the JDK. Each public static `String` method
answers one row, and `main` prints them (every `t_`, `h_` and `x_` method, sorted by name) as `label=result`, one per
line. `java25.txt` is that output, raw and unedited, from OpenJDK 25.0.5.1 (`/usr/lib/jvm/java-25-openjdk`). A call that
throws prints `EXC <class>:<message>`; none does. No jar is involved, so there is no jar SHA-256.

```sh
J=/usr/lib/jvm/java-25-openjdk/bin
$J/javac --release 21 -d classes demo/Probe.java
$J/java -XX:ActiveProcessorCount=1 -Xmx512m -cp classes demo.Probe > java25.txt
```

`java -version`:

```
openjdk version "25.0.5.1" 2026-11-17
OpenJDK Runtime Environment (build 25.0.5.1)
OpenJDK 64-Bit Server VM (build 25.0.5.1, mixed mode, sharing)
```

Two runs are byte-identical (`cmp`). Every printed character is printable ASCII: the JDK escapes the rest as `\uXXXX`.

Rows: `t_<label>` is `toString`, `h_<label>` is `hashCode` (left out for a `Class` or enum element, whose hash is an
identity hash code and differs between runs), `x_<label>` is `a.equals(b)/b.equals(a)/hashCodes agree`.

What the oracle shows, and the test pins:

- The name is the canonical one (`@demo.Probe.Inner.Deep`), a `Class` element prints its canonical name too
  (`java.lang.String[][].class`, `demo.Probe.Inner.class`), an enum constant prints bare (`A`, also for a constant with
  a body).
- A lone element named `value` prints without `value=`, whatever its type (`@demo.PAnno("marked")`, `VArr({"a"})`);
  any other lone element, or `value` beside another, prints `name=`.
- The order is the JDK's: the defaulted elements first, in the bucket order of the `HashMap` that holds an annotation
  type's defaults (a table of 2, 4, 8, 16, 32, 64 slots for 1, 2-3, 4-6, 7-12, 13-24, 25-48 defaults; `t_dflt*` rows),
  an explicit value replacing its default in place, then the elements without a default in the order the annotation
  lists them (`t_req3*`, `t_mix*`). The exploration that fixed the table sizes ran 148 generated types of 1 to 100
  defaulted elements.
- Escapes: `\b \t \n \f \r \" \\` by name, every other UTF-16 unit outside 0x20-0x7e as `\uXXXX` (lower case, a
  supplementary character as its two surrogates); `'` is escaped in a `char` and not in a `String`, `"` the other way.
- `byte` is `(byte)0x01`, `short` and `int` plain, `long` has `L`, `float` has `f`; NaN and the infinities are
  `0.0f/0.0f`, `1.0f/0.0f`, `-1.0f/0.0f` (and without `f` for `double`).
- `hashCode` is the sum of `127 * name.hashCode() ^ value hash`; `equals` is value equality with `Float`/`Double`
  rules (a NaN equals itself, `0.0` differs from `-0.0`).

What stays counted, and why (the test requires each to stay counted):

- Two defaulted elements in one bucket (`t_many*`, `t_dual*`): the JDK puts them in the order
  `getDeclaredMethods()` listed them, which HotSpot does not define. The oracle shows it was neither declaration order
  nor its reverse (`t_many0`: `lambda` before `iota`, `delta` before `mu`).
- A string with an unpaired surrogate, or a real U+FFFD (`t_str_lone*`, `t_str_fffd`, their `h_` rows): the class
  reader turns a lone surrogate into U+FFFD (#447), so the two cannot be told apart.
- An annotation type of the JDK (`t_jdk_deprecated`, `t_jdk_functional`): the runtime holds no class file for it, so
  it has no element list or defaults.
- `equals` of a `Class` or enum element (`x_class_*`, `x_enum_*`) keeps the earlier decision to count it.
- `String.valueOf(annotation)`, `"" + annotation` and `StringBuilder.append(annotation)` (`t_valueof`, `t_concat`)
  print through the host's handle model, which holds no class file: they were counted before and stay counted. A direct
  `annotation.toString()` is the exact path.
- `Method.getParameterAnnotations()` (`t_params`) is a separate counted call, unrelated to the text of an annotation.
