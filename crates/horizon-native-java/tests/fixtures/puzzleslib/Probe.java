package demo;

import com.google.common.collect.ImmutableMap;
import com.google.common.collect.Maps;
import com.google.common.collect.Queues;
import java.util.Map;
import java.util.Queue;
import java.util.concurrent.ConcurrentLinkedQueue;
import org.apache.commons.lang3.StringUtils;

public class Probe {
    enum Phase { ADD, REMOVE, MODIFY, POST, EXTRA1, EXTRA2, EXTRA3, EXTRA4 }

    public static String modId() {
        return StringUtils.isNotEmpty("vehicleupgrade") + "|"
            + StringUtils.isNotEmpty("") + "|"
            + StringUtils.isNotEmpty(null);
    }

    public static String queue() {
        Queue<String> queue = Queues.newConcurrentLinkedQueue();
        return queue.offer("one") + "|" + queue.poll() + "|" + queue.isEmpty();
    }

    public static String phase() {
        Map<Phase, String> source = Map.of(Phase.ADD, "add", Phase.REMOVE, "remove", Phase.MODIFY, "modify", Phase.POST, "post", Phase.EXTRA1, "extra1", Phase.EXTRA2, "extra2", Phase.EXTRA3, "extra3", Phase.EXTRA4, "extra4");
        ImmutableMap<Phase, String> copy = Maps.immutableEnumMap(source);
        return copy.get(Phase.ADD) + "|" + copy.get(Phase.POST);
    }
}
