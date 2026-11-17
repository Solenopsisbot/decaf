# Gson Reader oracle

Java 25 capture using Gson 2.14.0. From this directory:

    J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
    G=~/ava/scratch/vanilla-values/oracle/libraries/server/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar
    $J/javac --release 25 -cp "$G" -d . demo/Probe.java
    $J/java -cp ".:$G" demo.Probe

The resource input is `resource-config.json`; the probe writes `reader-config.json` with `Files.writeString` before opening it.
