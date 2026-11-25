# java.util.Base64 Java 25 capture

Captured with OpenJDK 25.0.5.1 (`java -version`: `openjdk version "25.0.5.1" 2026-11-17`,
`OpenJDK Runtime Environment (build 25.0.5.1)`,
`OpenJDK 64-Bit Server VM (build 25.0.5.1, mixed mode, sharing)`):

```
J=/usr/lib/jvm/java-25-openjdk/bin
$J/javac --release 21 -d classes demo/Probe.java
$J/java -cp classes demo.Probe > java25.txt
```

`java25.txt` is `Probe.rows()`, unedited:

* `idm`, `idd`, `types`, `cast`: which encoders and decoders are the same object. The five
  no-argument calls give one object each; `withoutPadding()` of a padded encoder is a new object
  every time and of an unpadded one the encoder itself; `getMimeEncoder(int, byte[])` is
  `getEncoder()` below four and a new object from four up.
* `enc`, `all`, `mimeline`, `big`, `fresh`, `alphabets`: `encode`/`encodeToString` of 0 to 24 and
  55 to 70 bytes and the 256 byte values, through the standard, URL and MIME encoders with and without padding
  (the MIME line breaks at 57 and 58 bytes, 114 to 118 bytes).
* `line`, `sep`, `sep-null`, `keep`: `getMimeEncoder(int, byte[])` with 8 line lengths and 5
  separators, the separator bytes the JDK refuses (the standard alphabet and `=`), a null
  separator, and that the encoder keeps the caller's separator array (a later write shows).
* `dst`, `dst-null`: `encode(byte[], byte[])`, destinations one short, exact and larger.
* `dec`, `dec4`, `decb`, `decb-fresh`: `decode(String)` and `decode(byte[])` through the three
  decoders: 145 named strings (every padding shape, white space, the other alphabet, chars above
  U+00FF, Latin-1 chars), all strings of up to three of `A + = - !` and of four of `A + =`, and
  byte arrays with bytes no string has.
* `rt`, `null`, `use`: round trips, null arguments, and the way a mod calls it (method
  references, a stream, a static and an instance field).

`differential/` is the larger comparison the rows are a sample of. `DecodeAll.java` prints the three
decoders' answer for every byte string up to a length over an alphabet, `EncodeAll.java` the
encoders' output for 191,052 inputs and configurations, and `Check.rs` compiles
`src/base64_codec.rs` alone and compares each line:

```
$J/javac -d dc differential/DecodeAll.java differential/EncodeAll.java
rustc -O --edition 2024 differential/Check.rs -o check
$J/java -cp dc DecodeAll 6 4151512b2d5f3d210aff80 > decode.txt && ./check decode decode.txt
$J/java -cp dc DecodeAll 7 41517a2f2b5f2d3d       > decode.txt && ./check decode decode.txt
$J/java -cp dc DecodeAll 9 413d210a2d             > decode.txt && ./check decode decode.txt
$J/java -cp dc DecodeAll 6 4151527a2f3d2b7f4c80e9 > decode.txt && ./check decode decode.txt
$J/java -cp dc EncodeAll > encode.txt && ./check encode encode.txt
```

The four decoder runs were 5,846,151, 7,190,235, 7,324,218 and 5,846,151 answers (3 decoders each
over every string), the encoder run 191,052; all equal. The `decode.txt` files are not committed.
