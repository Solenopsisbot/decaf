package demo;

import java.util.ArrayList;
import java.util.List;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.loader.api.FabricLoader;

/**
 * Which object `getEntrypoints` answers for each declaration (#669 review). Fabric Loader 0.19.5
 * (javap of `EntrypointStorage`): `add` makes a new `NewEntry` per declaration, and
 * `NewEntry.getOrCreate` caches one instance per requested type, so each declaration answers the
 * instance the loader built and ran for it, a class declared twice answers two, and a second
 * call answers the same ones. `Second` and `Dup` note each instance as its `onInitialize` runs.
 */
public final class EntrySelves {
    static final List<Object> SEEN = new ArrayList<>();

    public static final class Second implements ModInitializer {
        @Override
        public void onInitialize() {
            SEEN.add(this);
        }
    }

    public static final class Dup implements ModInitializer {
        @Override
        public void onInitialize() {
            SEEN.add(this);
        }
    }

    public static String selves() {
        List<ModInitializer> list = FabricLoader.getInstance().getEntrypoints("main", ModInitializer.class);
        List<ModInitializer> again = FabricLoader.getInstance().getEntrypoints("main", ModInitializer.class);
        StringBuilder out = new StringBuilder().append(list.size());
        for (int i = 0; i < list.size(); i++) {
            out.append('|').append(list.get(i) == SEEN.get(i)).append(list.get(i) == again.get(i));
        }
        return out.append('|').append(list.get(1) == list.get(2)).toString();
    }
}
