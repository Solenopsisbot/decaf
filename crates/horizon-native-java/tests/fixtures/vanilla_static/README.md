# vanilla_static Java 25 capture

Run from this fixture directory (`crates/horizon-native-java/tests/fixtures/vanilla_static`), with
`JAVA_HOME` pointing at a JDK 25.0.5 and `SERVER` at a folder holding the contents of the 26.3 server
bundler jar's `META-INF/` (`versions/26.3/server-26.3.jar` and `libraries/`). Never commit those jars.

```sh
CP=$SERVER/versions/26.3/server-26.3.jar:$(find "$SERVER/libraries" -name '*.jar' | tr '\n' ':')
mkdir classes
"$JAVA_HOME/bin/javac" -cp "$CP" -d classes demo/Probe.java oracle/Capture483.java oracle/CaptureCounted.java
"$JAVA_HOME/bin/java" -cp "classes:$CP" Capture483 > java25.txt
"$JAVA_HOME/bin/java" -cp "classes:$CP" CaptureCounted   # prints true: the fieldOf test's answer
```

The first field is `CommonComponents.EMPTY.getString()`, whose value is the empty string, so the capture begins with `|`.
