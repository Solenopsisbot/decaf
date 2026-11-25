package demo;

import java.util.*;
import java.util.function.*;

public final class Probe {
    interface Script { void run(ArrayDeque<Integer> d, Iterator<Integer> it, StringBuilder sb); }
    static ArrayDeque<Integer> small(int n) { List<Integer> l = new ArrayList<>(); for (int i = 1; i <= n; i++) l.add(i); return new ArrayDeque<>(l); }
    static ArrayDeque<Integer> roomy(int n) { ArrayDeque<Integer> d = new ArrayDeque<>(); for (int i = 1; i <= n; i++) d.add(i); return d; }
    static ArrayDeque<Integer> wrapped() { ArrayDeque<Integer> d = new ArrayDeque<>(4); d.add(3); d.add(4); d.addFirst(2); d.addFirst(1); return d; }
    static ArrayDeque<Integer> grown() { ArrayDeque<Integer> d = new ArrayDeque<>(2); d.add(1); d.add(2); d.addFirst(0); d.add(3); return d; }

    /// `witness` is made before the script runs and drained after it: it reads the live array from the old `head`, so it shows where the script left the elements.
    static String go(ArrayDeque<Integer> d, boolean descending, Script script) {
        Iterator<Integer> witness = descending ? d.descendingIterator() : d.iterator();
        Iterator<Integer> it = descending ? d.descendingIterator() : d.iterator();
        StringBuilder sb = new StringBuilder();
        try { script.run(d, it, sb); }
        catch (RuntimeException e) { sb.append(e.getClass().getSimpleName()).append(':').append(e.getMessage()); }
        sb.append('|').append(d).append('|');
        try { while (witness.hasNext()) sb.append(witness.next()).append(','); }
        catch (RuntimeException e) { sb.append(e.getClass().getSimpleName()); }
        return sb.toString();
    }

    public static String rm1Small4() { return go(small(4), false, (d, it, sb) -> { sb.append(it.next()); it.remove(); }); }
    public static String rm1Roomy6() { return go(roomy(6), false, (d, it, sb) -> { sb.append(it.next()); it.remove(); }); }
    public static String rm1Wrapped() { return go(wrapped(), false, (d, it, sb) -> { sb.append(it.next()); it.remove(); }); }
    public static String rm1Grown() { return go(grown(), false, (d, it, sb) -> { sb.append(it.next()); it.remove(); }); }
    public static String rm2Small4() { return go(small(4), false, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String rm2Roomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String rm2Wrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String rm2Grown() { return go(grown(), false, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String rm2Roomy4() { return go(roomy(4), false, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String rm2Small6() { return go(small(6), false, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String rmLastSmall4() { return go(small(4), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String rmLastRoomy6() { return go(roomy(6), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String rmLastWrapped() { return go(wrapped(), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String rmLastGrown() { return go(grown(), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String rmLastRoomy4() { return go(roomy(4), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String rmAllSmall4() { return go(small(4), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String rmAllRoomy6() { return go(roomy(6), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String rmAllWrapped() { return go(wrapped(), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String rmAllGrown() { return go(grown(), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String rmAllRoomy4() { return go(roomy(4), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String rmAllBig() { return go(roomy(40), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String rmAllEmpty() { return go(small(0), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String rmEvenSmall4() { return go(small(4), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String rmEvenRoomy6() { return go(roomy(6), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String rmEvenWrapped() { return go(wrapped(), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String rmEvenGrown() { return go(grown(), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String rmEvenSmall6() { return go(small(6), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String rmEvenRoomy4() { return go(roomy(4), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String rmEvenBig() { return go(roomy(40), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String rmEvenEmpty() { return go(small(0), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String rmOddSmall4() { return go(small(4), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String rmOddRoomy6() { return go(roomy(6), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String rmOddWrapped() { return go(wrapped(), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String rmOddGrown() { return go(grown(), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String rmOddRoomy4() { return go(roomy(4), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String rmOddBig() { return go(roomy(40), false, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String rmThirdSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThirdRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThirdWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThirdGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThirdRoomy4() { return go(roomy(4), false, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThirdSmall6() { return go(small(6), false, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThirdBig() { return go(roomy(40), false, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmBeforeNextSmall4() { return go(small(4), false, (d, it, sb) -> { it.remove(); }); }
    public static String rmBeforeNextRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.remove(); }); }
    public static String rmBeforeNextWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.remove(); }); }
    public static String rmBeforeNextGrown() { return go(grown(), false, (d, it, sb) -> { it.remove(); }); }
    public static String rmBeforeNextEmpty() { return go(small(0), false, (d, it, sb) -> { it.remove(); }); }
    public static String rmDoubleSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.remove(); it.remove(); }); }
    public static String rmDoubleRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.remove(); it.remove(); }); }
    public static String rmDoubleWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.remove(); it.remove(); }); }
    public static String rmDoubleGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.remove(); it.remove(); }); }
    public static String rmAfterHasNextSmall4() { return go(small(4), false, (d, it, sb) -> { it.hasNext(); it.remove(); }); }
    public static String rmAfterHasNextRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.hasNext(); it.remove(); }); }
    public static String rmAfterHasNextWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.hasNext(); it.remove(); }); }
    public static String rmAfterHasNextGrown() { return go(grown(), false, (d, it, sb) -> { it.hasNext(); it.remove(); }); }
    public static String rmNextRmSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.remove(); it.next(); it.remove(); }); }
    public static String rmNextRmRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.remove(); it.next(); it.remove(); }); }
    public static String rmNextRmWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.remove(); it.next(); it.remove(); }); }
    public static String rmNextRmGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.remove(); it.next(); it.remove(); }); }
    public static String rmThenAddFirstSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.addFirst(0); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenAddFirstRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.addFirst(0); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenAddFirstWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.addFirst(0); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenAddFirstGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.addFirst(0); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenAddSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.add(9); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenAddRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.add(9); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenAddWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.add(9); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenAddGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.add(9); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollFirstSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollFirstRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollFirstWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollFirstGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollFirstRoomy4() { return go(roomy(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollLastSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollLast(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollLastRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollLast(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollLastWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollLast(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenPollLastGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollLast(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenClearSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.remove(); d.clear(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenClearRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.remove(); d.clear(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenClearWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.remove(); d.clear(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenClearGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.remove(); d.clear(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmAllThenNextSmall4() { return go(small(4), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } it.next(); }); }
    public static String rmAllThenNextRoomy6() { return go(roomy(6), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } it.next(); }); }
    public static String rmAllThenNextWrapped() { return go(wrapped(), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } it.next(); }); }
    public static String rmAllThenNextGrown() { return go(grown(), false, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } it.next(); }); }
    public static String rmThenNextPastEndSmall4() { return go(small(4), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); it.next(); }); }
    public static String rmThenNextPastEndRoomy6() { return go(roomy(6), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); it.next(); }); }
    public static String rmThenNextPastEndWrapped() { return go(wrapped(), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); it.next(); }); }
    public static String rmThenNextPastEndGrown() { return go(grown(), false, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); it.next(); }); }
    public static String rmAfterEachSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachDoneSmall4() { return go(small(4), false, (d, it, sb) -> { while (it.hasNext()) sb.append(it.next()); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachDoneRoomy6() { return go(roomy(6), false, (d, it, sb) -> { while (it.hasNext()) sb.append(it.next()); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachDoneWrapped() { return go(wrapped(), false, (d, it, sb) -> { while (it.hasNext()) sb.append(it.next()); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachDoneGrown() { return go(grown(), false, (d, it, sb) -> { while (it.hasNext()) sb.append(it.next()); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachAfterRmSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachAfterRmRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachAfterRmWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachAfterRmGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachEmptySmall4() { return go(small(4), false, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachEmptyRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachEmptyWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachEmptyGrown() { return go(grown(), false, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachEmptyEmpty() { return go(small(0), false, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachZeroSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachZeroRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachZeroWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachZeroGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String rmAfterEachZeroOne() { return go(small(1), false, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String stalePollFirstSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); d.pollFirst(); it.remove(); }); }
    public static String stalePollFirstRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); d.pollFirst(); it.remove(); }); }
    public static String stalePollFirstWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); d.pollFirst(); it.remove(); }); }
    public static String stalePollFirstGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); d.pollFirst(); it.remove(); }); }
    public static String stalePollLastSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); d.pollLast(); it.remove(); }); }
    public static String stalePollLastRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); d.pollLast(); it.remove(); }); }
    public static String stalePollLastWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); d.pollLast(); it.remove(); }); }
    public static String stalePollLastGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); d.pollLast(); it.remove(); }); }
    public static String stalePollLast2Small4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); d.pollLast(); it.remove(); }); }
    public static String stalePollLast2Roomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); d.pollLast(); it.remove(); }); }
    public static String stalePollLast2Wrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); d.pollLast(); it.remove(); }); }
    public static String stalePollLast2Grown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); d.pollLast(); it.remove(); }); }
    public static String staleAddFirstSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); d.addFirst(0); it.remove(); }); }
    public static String staleAddFirstRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); d.addFirst(0); it.remove(); }); }
    public static String staleAddFirstWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); d.addFirst(0); it.remove(); }); }
    public static String staleAddFirstGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); d.addFirst(0); it.remove(); }); }
    public static String staleAddSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); d.add(9); it.remove(); }); }
    public static String staleAddRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); d.add(9); it.remove(); }); }
    public static String staleAddWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); d.add(9); it.remove(); }); }
    public static String staleAddGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); d.add(9); it.remove(); }); }
    public static String staleClearSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); d.clear(); it.remove(); }); }
    public static String staleClearRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); d.clear(); it.remove(); }); }
    public static String staleClearWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); d.clear(); it.remove(); }); }
    public static String staleClearGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); d.clear(); it.remove(); }); }
    public static String stalePollFirst2Small4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); }); }
    public static String stalePollFirst2Roomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); }); }
    public static String stalePollFirst2Wrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); }); }
    public static String stalePollFirst2Grown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); }); }
    public static String staleRemoveOtherSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); d.remove(1); it.remove(); }); }
    public static String staleRemoveOtherRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); d.remove(1); it.remove(); }); }
    public static String staleRemoveOtherWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); d.remove(1); it.remove(); }); }
    public static String staleRemoveOtherGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); d.remove(1); it.remove(); }); }
    public static String otherIteratorSmall4() { return go(small(4), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String otherIteratorRoomy6() { return go(roomy(6), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String otherIteratorWrapped() { return go(wrapped(), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String otherIteratorGrown() { return go(grown(), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String otherIteratorRoomy4() { return go(roomy(4), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String otherIteratorSmall6() { return go(small(6), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String otherIteratorLastSmall4() { return go(small(4), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); while (o.hasNext()) o.next(); o.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String otherIteratorLastRoomy6() { return go(roomy(6), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); while (o.hasNext()) o.next(); o.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String otherIteratorLastWrapped() { return go(wrapped(), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); while (o.hasNext()) o.next(); o.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String otherIteratorLastGrown() { return go(grown(), false, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); while (o.hasNext()) o.next(); o.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String rmThenStaleRmSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); it.remove(); }); }
    public static String rmThenStaleRmRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); it.remove(); }); }
    public static String rmThenStaleRmWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); it.remove(); }); }
    public static String rmThenStaleRmGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); it.remove(); }); }
    public static String rmThenForEachSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String rmThenForEachRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String rmThenForEachWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String rmThenForEachGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String rmThenForEachRoomy4() { return go(roomy(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String rmThenForEachSmall6() { return go(small(6), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String rmThenLoopSmall4() { return go(small(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String rmThenLoopRoomy6() { return go(roomy(6), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String rmThenLoopWrapped() { return go(wrapped(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String rmThenLoopGrown() { return go(grown(), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String rmThenLoopRoomy4() { return go(roomy(4), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String rmThenLoopSmall6() { return go(small(6), false, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String descrm1Small4() { return go(small(4), true, (d, it, sb) -> { sb.append(it.next()); it.remove(); }); }
    public static String descrm1Roomy6() { return go(roomy(6), true, (d, it, sb) -> { sb.append(it.next()); it.remove(); }); }
    public static String descrm1Wrapped() { return go(wrapped(), true, (d, it, sb) -> { sb.append(it.next()); it.remove(); }); }
    public static String descrm1Grown() { return go(grown(), true, (d, it, sb) -> { sb.append(it.next()); it.remove(); }); }
    public static String descrm2Small4() { return go(small(4), true, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String descrm2Roomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String descrm2Wrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String descrm2Grown() { return go(grown(), true, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String descrm2Roomy4() { return go(roomy(4), true, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String descrm2Small6() { return go(small(6), true, (d, it, sb) -> { it.next(); sb.append(it.next()); it.remove(); sb.append(it.next()); }); }
    public static String descrmLastSmall4() { return go(small(4), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String descrmLastRoomy6() { return go(roomy(6), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String descrmLastWrapped() { return go(wrapped(), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String descrmLastGrown() { return go(grown(), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String descrmLastRoomy4() { return go(roomy(4), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); sb.append(it.hasNext()); }); }
    public static String descrmAllSmall4() { return go(small(4), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String descrmAllRoomy6() { return go(roomy(6), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String descrmAllWrapped() { return go(wrapped(), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String descrmAllGrown() { return go(grown(), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String descrmAllRoomy4() { return go(roomy(4), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String descrmAllBig() { return go(roomy(40), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String descrmAllEmpty() { return go(small(0), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } }); }
    public static String descrmEvenSmall4() { return go(small(4), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String descrmEvenRoomy6() { return go(roomy(6), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String descrmEvenWrapped() { return go(wrapped(), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String descrmEvenGrown() { return go(grown(), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String descrmEvenSmall6() { return go(small(6), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String descrmEvenRoomy4() { return go(roomy(4), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String descrmEvenBig() { return go(roomy(40), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String descrmEvenEmpty() { return go(small(0), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 == 0) it.remove(); } }); }
    public static String descrmOddSmall4() { return go(small(4), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String descrmOddRoomy6() { return go(roomy(6), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String descrmOddWrapped() { return go(wrapped(), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String descrmOddGrown() { return go(grown(), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String descrmOddRoomy4() { return go(roomy(4), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String descrmOddBig() { return go(roomy(40), true, (d, it, sb) -> { while (it.hasNext()) { if (it.next() % 2 != 0) it.remove(); } }); }
    public static String descrmThirdSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThirdRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThirdWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThirdGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThirdRoomy4() { return go(roomy(4), true, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThirdSmall6() { return go(small(6), true, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThirdBig() { return go(roomy(40), true, (d, it, sb) -> { it.next(); it.next(); it.next(); it.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmBeforeNextSmall4() { return go(small(4), true, (d, it, sb) -> { it.remove(); }); }
    public static String descrmBeforeNextRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.remove(); }); }
    public static String descrmBeforeNextWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.remove(); }); }
    public static String descrmBeforeNextGrown() { return go(grown(), true, (d, it, sb) -> { it.remove(); }); }
    public static String descrmBeforeNextEmpty() { return go(small(0), true, (d, it, sb) -> { it.remove(); }); }
    public static String descrmDoubleSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.remove(); it.remove(); }); }
    public static String descrmDoubleRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.remove(); it.remove(); }); }
    public static String descrmDoubleWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.remove(); it.remove(); }); }
    public static String descrmDoubleGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.remove(); it.remove(); }); }
    public static String descrmAfterHasNextSmall4() { return go(small(4), true, (d, it, sb) -> { it.hasNext(); it.remove(); }); }
    public static String descrmAfterHasNextRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.hasNext(); it.remove(); }); }
    public static String descrmAfterHasNextWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.hasNext(); it.remove(); }); }
    public static String descrmAfterHasNextGrown() { return go(grown(), true, (d, it, sb) -> { it.hasNext(); it.remove(); }); }
    public static String descrmNextRmSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.remove(); it.next(); it.remove(); }); }
    public static String descrmNextRmRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.remove(); it.next(); it.remove(); }); }
    public static String descrmNextRmWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.remove(); it.next(); it.remove(); }); }
    public static String descrmNextRmGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.remove(); it.next(); it.remove(); }); }
    public static String descrmThenAddFirstSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.addFirst(0); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenAddFirstRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.addFirst(0); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenAddFirstWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.addFirst(0); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenAddFirstGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.addFirst(0); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenAddSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.add(9); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenAddRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.add(9); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenAddWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.add(9); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenAddGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.add(9); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollFirstSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollFirstRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollFirstWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollFirstGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollFirstRoomy4() { return go(roomy(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollFirst(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollLastSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollLast(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollLastRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollLast(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollLastWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollLast(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenPollLastGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.pollLast(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenClearSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.remove(); d.clear(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenClearRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.remove(); d.clear(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenClearWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.remove(); d.clear(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenClearGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.remove(); d.clear(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmAllThenNextSmall4() { return go(small(4), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } it.next(); }); }
    public static String descrmAllThenNextRoomy6() { return go(roomy(6), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } it.next(); }); }
    public static String descrmAllThenNextWrapped() { return go(wrapped(), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } it.next(); }); }
    public static String descrmAllThenNextGrown() { return go(grown(), true, (d, it, sb) -> { while (it.hasNext()) { it.next(); it.remove(); } it.next(); }); }
    public static String descrmThenNextPastEndSmall4() { return go(small(4), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); it.next(); }); }
    public static String descrmThenNextPastEndRoomy6() { return go(roomy(6), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); it.next(); }); }
    public static String descrmThenNextPastEndWrapped() { return go(wrapped(), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); it.next(); }); }
    public static String descrmThenNextPastEndGrown() { return go(grown(), true, (d, it, sb) -> { while (it.hasNext()) it.next(); it.remove(); it.next(); }); }
    public static String descrmAfterEachSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachDoneSmall4() { return go(small(4), true, (d, it, sb) -> { while (it.hasNext()) sb.append(it.next()); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachDoneRoomy6() { return go(roomy(6), true, (d, it, sb) -> { while (it.hasNext()) sb.append(it.next()); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachDoneWrapped() { return go(wrapped(), true, (d, it, sb) -> { while (it.hasNext()) sb.append(it.next()); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachDoneGrown() { return go(grown(), true, (d, it, sb) -> { while (it.hasNext()) sb.append(it.next()); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachAfterRmSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachAfterRmRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachAfterRmWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachAfterRmGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachEmptySmall4() { return go(small(4), true, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachEmptyRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachEmptyWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachEmptyGrown() { return go(grown(), true, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachEmptyEmpty() { return go(small(0), true, (d, it, sb) -> { it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachZeroSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachZeroRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachZeroWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachZeroGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descrmAfterEachZeroOne() { return go(small(1), true, (d, it, sb) -> { it.next(); it.remove(); it.forEachRemaining(x -> sb.append(x)); it.remove(); }); }
    public static String descstalePollFirstSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); d.pollFirst(); it.remove(); }); }
    public static String descstalePollFirstRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); d.pollFirst(); it.remove(); }); }
    public static String descstalePollFirstWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); d.pollFirst(); it.remove(); }); }
    public static String descstalePollFirstGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); d.pollFirst(); it.remove(); }); }
    public static String descstalePollLastSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); d.pollLast(); it.remove(); }); }
    public static String descstalePollLastRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); d.pollLast(); it.remove(); }); }
    public static String descstalePollLastWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); d.pollLast(); it.remove(); }); }
    public static String descstalePollLastGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); d.pollLast(); it.remove(); }); }
    public static String descstalePollLast2Small4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); d.pollLast(); it.remove(); }); }
    public static String descstalePollLast2Roomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); d.pollLast(); it.remove(); }); }
    public static String descstalePollLast2Wrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); d.pollLast(); it.remove(); }); }
    public static String descstalePollLast2Grown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); d.pollLast(); it.remove(); }); }
    public static String descstaleAddFirstSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); d.addFirst(0); it.remove(); }); }
    public static String descstaleAddFirstRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); d.addFirst(0); it.remove(); }); }
    public static String descstaleAddFirstWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); d.addFirst(0); it.remove(); }); }
    public static String descstaleAddFirstGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); d.addFirst(0); it.remove(); }); }
    public static String descstaleAddSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); d.add(9); it.remove(); }); }
    public static String descstaleAddRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); d.add(9); it.remove(); }); }
    public static String descstaleAddWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); d.add(9); it.remove(); }); }
    public static String descstaleAddGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); d.add(9); it.remove(); }); }
    public static String descstaleClearSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); d.clear(); it.remove(); }); }
    public static String descstaleClearRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); d.clear(); it.remove(); }); }
    public static String descstaleClearWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); d.clear(); it.remove(); }); }
    public static String descstaleClearGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); d.clear(); it.remove(); }); }
    public static String descstalePollFirst2Small4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); }); }
    public static String descstalePollFirst2Roomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); }); }
    public static String descstalePollFirst2Wrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); }); }
    public static String descstalePollFirst2Grown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); }); }
    public static String descstaleRemoveOtherSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); d.remove(1); it.remove(); }); }
    public static String descstaleRemoveOtherRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); d.remove(1); it.remove(); }); }
    public static String descstaleRemoveOtherWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); d.remove(1); it.remove(); }); }
    public static String descstaleRemoveOtherGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); d.remove(1); it.remove(); }); }
    public static String descotherIteratorSmall4() { return go(small(4), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String descotherIteratorRoomy6() { return go(roomy(6), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String descotherIteratorWrapped() { return go(wrapped(), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String descotherIteratorGrown() { return go(grown(), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String descotherIteratorRoomy4() { return go(roomy(4), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String descotherIteratorSmall6() { return go(small(6), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); o.next(); o.next(); o.remove(); sb.append(it.next()); sb.append(it.next()); }); }
    public static String descotherIteratorLastSmall4() { return go(small(4), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); while (o.hasNext()) o.next(); o.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descotherIteratorLastRoomy6() { return go(roomy(6), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); while (o.hasNext()) o.next(); o.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descotherIteratorLastWrapped() { return go(wrapped(), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); while (o.hasNext()) o.next(); o.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descotherIteratorLastGrown() { return go(grown(), true, (d, it, sb) -> { Iterator<Integer> o = d.iterator(); while (o.hasNext()) o.next(); o.remove(); while (it.hasNext()) sb.append(it.next()); }); }
    public static String descrmThenStaleRmSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); it.remove(); }); }
    public static String descrmThenStaleRmRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); it.remove(); }); }
    public static String descrmThenStaleRmWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); it.remove(); }); }
    public static String descrmThenStaleRmGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); d.pollFirst(); it.remove(); it.remove(); }); }
    public static String descrmThenForEachSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String descrmThenForEachRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String descrmThenForEachWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String descrmThenForEachGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String descrmThenForEachRoomy4() { return go(roomy(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String descrmThenForEachSmall6() { return go(small(6), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); d.forEach(x -> { sb.append(x); if (x != null && x == 3) d.pollLast(); }); }); }
    public static String descrmThenLoopSmall4() { return go(small(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String descrmThenLoopRoomy6() { return go(roomy(6), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String descrmThenLoopWrapped() { return go(wrapped(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String descrmThenLoopGrown() { return go(grown(), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String descrmThenLoopRoomy4() { return go(roomy(4), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }
    public static String descrmThenLoopSmall6() { return go(small(6), true, (d, it, sb) -> { it.next(); it.next(); it.remove(); for (Integer x : d) { sb.append(x); if (x == 3) d.pollLast(); } }); }

    public static void main(String[] args) throws Exception {
        java.lang.reflect.Method[] all = Probe.class.getDeclaredMethods();
        Arrays.sort(all, Comparator.comparing(java.lang.reflect.Method::getName));
        for (java.lang.reflect.Method m : all) {
            if (!java.lang.reflect.Modifier.isPublic(m.getModifiers()) || m.getParameterCount() != 0 || m.getReturnType() != String.class || m.getName().equals("main")) continue;
            String r;
            try { r = (String) m.invoke(null); } catch (Throwable t) { r = "THROW " + t; }
            System.out.println(m.getName() + "=" + r);
        }
    }
}
