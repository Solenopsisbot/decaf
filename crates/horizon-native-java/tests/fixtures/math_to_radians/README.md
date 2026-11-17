# Math.toRadians Java 25 capture

Run from this fixture directory:

```sh
J="$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin"
rm -rf classes && mkdir classes
"$J/javac" --release 21 -d classes demo/Probe.java
"$J/java" -cp classes demo.Probe
"$J/java" -version
```

The probe calls `java.lang.Math.toRadians(double)` for signed zero, finite,
NaN, infinities, and the smallest positive subnormal. Java 25's implementation
is `angdeg * 0.017453292519943295`, confirmed with `javap -c -p java.lang.Math`.

The nanBits row is counted because raw NaN payload bits are CPU-dependent (ARM and x86 HotSpot may differ).
