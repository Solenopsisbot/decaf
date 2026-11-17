package oracle;

import java.net.URI;
import java.nio.file.FileSystem;
import java.nio.file.FileSystems;
import java.lang.reflect.Proxy;
import java.nio.file.Path;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import net.fabricmc.loader.api.ModContainer;
import net.fabricmc.loader.api.Version;
import net.fabricmc.loader.api.metadata.ModMetadata;
import net.fabricmc.loader.api.metadata.ModOrigin;

/**
 * Java 25 side only (never in the Decaf jar): runs `demo.Probe.rows(container)` with a container
 * whose single root is the zip file system of `args[0]`, a jar of the fixture's `resources/`, the
 * way Fabric Loader roots a jar mod.
 */
public final class OracleMain {
    public static void main(String[] args) throws Exception {
        FileSystem fs = FileSystems.newFileSystem(URI.create("jar:" + Path.of(args[0]).toUri()), Map.of());
        Path root = fs.getPath("/");
        System.out.print(demo.Probe.rows(new Container(root)));
    }

    @SuppressWarnings("unchecked")
    static <T> T proxy(Class<T> type, Map<String, Object> answers) {
        return (T) Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[] {type}, (self, method, args) -> {
            if (!answers.containsKey(method.getName())) {
                throw new UnsupportedOperationException(method.getName());
            }
            return answers.get(method.getName());
        });
    }

    record Container(Path root) implements ModContainer {
        public List<Path> getRootPaths() { return List.of(root); }
        /** What `ModNioPackResources.create` reads: the name and the version's friendly string. */
        public ModMetadata getMetadata() {
            Version version = proxy(Version.class, Map.of("getFriendlyString", "1"));
            return proxy(ModMetadata.class, Map.of("getId", "demo", "getName", "demo", "getVersion", version));
        }
        public ModOrigin getOrigin() { throw new UnsupportedOperationException(); }
        public Optional<ModContainer> getContainingMod() { return Optional.empty(); }
        public Collection<ModContainer> getContainedMods() { return List.of(); }
        public Path getRootPath() { return root; }
        public Path getPath(String file) { return root.resolve(file); }
    }
}
