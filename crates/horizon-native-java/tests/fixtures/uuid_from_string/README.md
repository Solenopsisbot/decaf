# `UUID.fromString` Java 25 capture

`java25.txt` is the output of a real JDK 25 run (OpenJDK 25.0.5.1), captured from this folder:
```
javac -d out demo/Probe.java && java -cp out demo.Probe > java25.txt
```
Each row is one input in brackets, then `ok <uuid> <msb> <lsb>` or the exception's class and
message. Inputs a runtime counts instead of answering (a non-ASCII field past the dash check, a
null name) are not in the probe; `fixture_uuid_from_string.rs` checks that they are counted.
