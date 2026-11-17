`demo/Probe.java` on OpenJDK 25.0.5 (plain JDK, two runs identical) prints `java25.txt`:

    javac --release 21 -d classes demo/Probe.java
    java -cp classes demo.Probe > java25.txt

On main before this fixture, every row but the two `declaring` rows already matched: a jar's
enum constants are ordinary objects with their real class (`E$1` for a constant with a body).
`Enum.getDeclaringClass()` was counted; Balm reads it.
