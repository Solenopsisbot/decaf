package demo;

import java.util.ArrayList;
import java.util.LinkedList;
import java.util.List;

/**
 * A fresh list and a sub-list of it per call, each asked its class: what a pouch tooltip's
 * per-frame {@code subList} does (#609 round 5). The host keeps each sub-list's class for
 * {@code getClass}; it must go with the list once a collection frees it.
 */
public final class SubListTick {
    private SubListTick() {}

    public static String tick() {
        List<Integer> array = new ArrayList<>(List.of(1, 2, 3));
        List<Integer> arraySub = array.subList(0, 2);
        List<Integer> linked = new LinkedList<>(List.of(4, 5, 6));
        List<Integer> linkedSub = linked.subList(1, 3);
        return arraySub.getClass().getName()
                + "|" + linkedSub.getClass().getName()
                + "|" + arraySub.size() + "," + linkedSub.get(0);
    }

    public static void main(String[] args) {
        System.out.println("subListTick=" + tick());
    }
}
