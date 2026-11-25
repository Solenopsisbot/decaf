# `castore`, `sastore` and `bastore` narrow what they store (fixture `array_store_narrow`)

JVMS 6.5: `castore` and `sastore` store the low 16 bits of the int on the stack (`char[]` zero-extends on load,
`short[]` sign-extends), `bastore` the low 8 bits for a `byte[]` and, on HotSpot, the low bit for a `boolean[]`.
javac narrows first (`i2c`, `i2s`, `i2b`), so nothing it emits shows the difference; a hand-written class file or
generated code can store a wide int. The runtime stored it whole into `char[]` / `short[]` (a review probe,
`ldc 0x123456; castore; caload`, gave 3 on Java 25 and the full int here: Java returns 13398 = 0x3456).
`byte[]` and `boolean[]` stores already narrowed.

`demo/Raw.java` has four straight-line methods (`charStore`, `shortStore`, `byteStore`, `boolStore`): store the
int argument into a one-element array, return the element. The fixture turns `i2c castore`, `i2s sastore` and
`i2b bastore` into `nop <store>` (the instruction offsets do not move, and there is no branch, so no
StackMapTable changes), and `boolStore`'s `newarray byte` (the second one in the class file) into `newarray
boolean`. `Run.java` makes the same byte edits (and checks the match counts: 1, 1, 2, 2), loads the patched
class in its own class loader and prints the result of every `demo/Probe` method; the test
(`fixture_array_store_narrow.rs`) applies the same edits to the javac output before packing the jar.
`demo/Probe.java` feeds 25 values from a static array: 0, 1, 2, 3, 127, 128, 255, 256, 257, 32767, 32768, 65535,
65536, 65537, 0x123456, -1, -2, -128, -129, -32768, -32769, Integer.MIN_VALUE, Integer.MAX_VALUE, 0x12345678,
0x7fff8000. Each method's line is the 25 results separated by spaces.

`java25.txt` was captured on 2026-11-23 (the demo classes and `Run` must not share a class path, or the class
loader's parent loads the unpatched `Raw`):

```sh
export JAVA_HOME=/usr/lib/jvm/java-25-openjdk
$JAVA_HOME/bin/javac --release 21 -d out demo/Raw.java demo/Probe.java
$JAVA_HOME/bin/javac -d run Run.java
$JAVA_HOME/bin/java -XX:ActiveProcessorCount=1 -Xmx512m -cp run Run out > java25.txt
```

`java -version`: openjdk version "25.0.5.1" 2026-11-17, OpenJDK Runtime Environment (build 25.0.5.1). The output
is the same with `-Xint` and with `-Xcomp`. `java25.txt` SHA-256:
`4061361f01e671e39581b81f8db9b7f8c30d926a53cfd1b8999bcaded5fefe83`. No library jar is involved.

`cargo test -p horizon-native-java --test fixture_array_store_narrow`: before the fix `charStore` and `shortStore`
differ (the full int is kept); `byteStore` and `boolStore` match.
