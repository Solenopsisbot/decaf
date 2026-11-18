package fuzs.puzzleslib.common.api.core.v1; import java.util.function.Supplier; public interface ModConstructor { static void construct(String id, Supplier<ModConstructor> s) {} }
