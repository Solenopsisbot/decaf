Java 25 capture:

J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
CP=$HOME/ava/scratch/vanilla-values/oracle/libraries/server/org/apache/logging/log4j/log4j-api/2.26.0/log4j-api-2.26.0.jar:$HOME/ava/scratch/vanilla-values/oracle/libraries/server/org/apache/logging/log4j/log4j-core/2.26.0/log4j-core-2.26.0.jar
$J/javac -cp "$CP" -d /tmp/lmf Probe.java
$J/java -cp "/tmp/lmf:$CP" demo.Probe

Java 25.0.5+8
