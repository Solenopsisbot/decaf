package demo;

import java.util.function.Consumer;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// A generic Consumer whose first bound supplies the event class, as typetools does.
public final class BoundedTickConsumer<T extends ServerTickEvent.Post>
        implements Consumer<T> {
    @Override
    public void accept(T event) {
        System.out.println("bounded");
    }
}
