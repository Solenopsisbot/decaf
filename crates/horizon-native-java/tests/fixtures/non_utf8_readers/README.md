# non_utf8_readers

Java 25 capture (`jdk-25.0.5+8`).

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin; rm -rf /tmp/nuclass; mkdir /tmp/nuclass; $J/javac -d /tmp/nuclass demo/Probe.java; printf '%s\n' 'import demo.Probe; public class Run { public static void main(String[]x){System.out.print(Probe.rows());}}' >/tmp/Run.java; $J/javac -cp /tmp/nuclass -d /tmp/nuclass /tmp/Run.java; $J/java -cp /tmp/nuclass Run
```
