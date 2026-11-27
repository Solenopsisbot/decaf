# `Item.canFitInsideContainerItems()` of every vanilla 26.3 item

`src/vanilla_can_fit_inside_container_items_java25.txt` is the output of `CanFitOracle.java`, run on JDK 25.0.5 against the 26.3 server jar
and its bundled libraries, with no network (`unshare -rn`):

```
CP="server-26.3.jar:<every META-INF/libraries jar>"
javac -cp "$CP" -d out CanFitOracle.java
unshare -rn java -cp "$CP:out" CanFitOracle   # the [STDOUT] payloads
```

It lists each registered item answering `false` (with its class) and the registry's size.
`src/vanilla_item_data.rs` (`CANNOT_FIT_INSIDE_CONTAINER_ITEMS`) is this list; its unit test
checks the two agree. (The capture sits in `src/` so `tools/vendor.py` can carry it into `mod/`.)
