# SharedConstants current version

```sh
. ~/ava/env.sh
mkdir -p /tmp/shared-constants-version-capture
CP=$HOME/ava/minecraft-26.3-server-game.jar
for j in $(find $HOME/ava/scratch/vanilla-values/oracle/libraries/server -name '*.jar'); do CP=$CP:$j; done
javac -cp "$CP" -d /tmp/shared-constants-version-capture demo/Probe.java oracle/Capture.java
java -cp /tmp/shared-constants-version-capture:$CP Capture
```
