# string_get_bytes

Captured with Java 25.0.5+8:

    J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
    $J/javac -d /tmp/sgb-classes demo/Probe.java
    $J/jshell --class-path /tmp/sgb-classes
    System.out.print(demo.Probe.rows());
