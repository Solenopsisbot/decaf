Compile and capture on Java 25:

    javac --release 21 -d out demo/Probe.java
    java -cp out demo.Probe

`javap -c -p demo.Probe` must show `wide` load/store and `wide iinc` instructions. Captured with Temurin 25.0.5+8.
