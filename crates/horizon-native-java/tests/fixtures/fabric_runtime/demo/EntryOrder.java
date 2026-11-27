package demo;

import java.util.ArrayList;
import java.util.List;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.loader.api.FabricLoader;

/**
 * When an entrypoint's constructor runs (#669 re-review): each instance logs its construction
 * and its `onInitialize`, which asks `getEntrypoints` for the kind. Declared twice, Fabric
 * builds the second instance inside the first one's query (`NewEntry.getOrCreate`), once.
 */
public final class EntryOrder implements ModInitializer {
    static final List<String> LOG = new ArrayList<>();
    static int made;
    final int id;

    public EntryOrder() {
        id = made++;
        LOG.add("new" + id);
    }

    @Override
    public void onInitialize() {
        LOG.add("init" + id);
        List<ModInitializer> all = FabricLoader.getInstance().getEntrypoints("main", ModInitializer.class);
        LOG.add("saw" + all.size() + (all.get(0) == this ? "self" : "other"));
    }

    public static String log() {
        return String.join(",", LOG) + "|" + made;
    }
}
