package demo;

import java.util.function.Consumer;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

abstract class GenericTickConsumer<T extends ServerTickEvent.Post> implements Consumer<T> {
    public void accept(T event) { System.out.println("generic"); }
}
final class ChildGenericTickConsumer extends GenericTickConsumer<ServerTickEvent.Post> {}
final class ViaVarTickConsumer<U extends ServerTickEvent.Post, T extends U> implements Consumer<T> {
    public void accept(T event) { System.out.println("via-var"); }
}
final class ErasedConsumer implements Consumer {
    public void accept(Object event) { System.out.println("erased"); }
}
