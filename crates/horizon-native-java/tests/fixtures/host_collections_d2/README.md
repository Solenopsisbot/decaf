# host_collections_d2 oracle provenance

Each capture is Java 25.0.5+8 (Temurin), compiled with `javac --release 21`
and run with `java -cp .` from the corresponding probe directory. `java25.txt`
is produced by `demo/Probe.java`; `review25.txt` by `demo/Review.java` with the
committed Gson 2.14.0 and Guava 33.6.0-jre stubs; `recheck25.txt` by
`demo/Recheck.java` with the same stubs; and `inv25.txt` by `demo/Inv.java`
with the committed client fakes and `NonNullList` stub. The exact source files
are adjacent to each capture and the Rust tests invoke the same methods.
Every `name=value` row in each capture is consumed by a named case in
`fixture_host_collections_d2.rs`; counted cases are explicitly listed and must
produce the corresponding counted host call rather than silently accepting a
value.
