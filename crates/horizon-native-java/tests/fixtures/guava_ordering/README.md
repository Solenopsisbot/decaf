`demo/Probe.java` on OpenJDK 25.0.5 (two runs identical) against Guava 33.6.0-jre
(SHA-256 `dc573e1fca4fd5454f4a5fd3d7da2df03002876a4175bafc14a95980dd7713b3`, the server
library jar of 26.3) prints `java25.txt`:

    javac --release 21 -cp guava-33.6.0-jre.jar -d classes demo/Probe.java
    java -cp classes:guava-33.6.0-jre.jar demo.Probe > java25.txt

`reverse.null` carries the JVM's helpful NullPointerException text: Guava's
`ReverseNaturalOrdering.compare` checks only `left`, then calls `right.compareTo(left)`.

`toString()` is not in the probe: the interpreter counts it on every callback handle,
these orderings included (`callback_marker::CALLBACK_TEXT`). jamlib only compares.
