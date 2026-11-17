import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;

public final class FilesProbe {
    static String attempt(RunnableWithIo op) {
        try { op.run(); return "ok"; }
        catch (Exception e) { return e.getClass().getSimpleName() + ":" + e.getMessage(); }
    }
    interface RunnableWithIo { void run() throws Exception; }
    public static void main(String[] args) throws Exception { System.out.print(rows()); }
    public static String rows() throws Exception {
        Path d = Path.of("game/files-vfs");
        Path sub = d.resolve("sub");
        Path p = sub.resolve("x.txt");
        List<String> r = new ArrayList<>();
        r.add("dir0=" + Files.exists(sub, LinkOption.NOFOLLOW_LINKS));
        Files.createDirectories(sub);
        r.add("dir1=" + Files.exists(sub, LinkOption.NOFOLLOW_LINKS)
            + ",isDir=" + Files.isDirectory(sub, LinkOption.NOFOLLOW_LINKS)
            + ",isReg=" + Files.isRegularFile(sub, LinkOption.NOFOLLOW_LINKS)
            + ",read=" + Files.isReadable(sub) + ",sym=" + Files.isSymbolicLink(sub)
            + ",exec=" + Files.isExecutable(sub));
        Files.writeString(p, "hello");
        r.add("file=" + Files.exists(p, LinkOption.NOFOLLOW_LINKS)
            + ",isDir=" + Files.isDirectory(p, LinkOption.NOFOLLOW_LINKS)
            + ",isReg=" + Files.isRegularFile(p, LinkOption.NOFOLLOW_LINKS)
            + ",read=" + Files.isReadable(p) + ",sym=" + Files.isSymbolicLink(p)
            + ",exec=" + Files.isExecutable(p) + ",text=" + Files.readString(p));
        r.add("list=" + Files.list(sub).count());
        Files.delete(p);
        r.add("afterDelete=" + Files.exists(p, LinkOption.NOFOLLOW_LINKS) + ",not=" + Files.notExists(p));
        Path missing = d.resolve("missing.txt");
        r.add("missing=" + attempt(() -> Files.readString(missing)));
        Path existing = d.resolve("existing.txt");
        Files.writeString(existing, "a");
        r.add("createNew=" + attempt(() -> Files.writeString(existing, "b", StandardOpenOption.CREATE_NEW)));
        Files.writeString(existing, "b", StandardOpenOption.APPEND);
        r.add("append=" + Files.readString(existing));
        r.add("truncateMissing=" + attempt(() -> Files.writeString(d.resolve("truncate.txt"), "x", StandardOpenOption.TRUNCATE_EXISTING)));
        Path appendMissing = d.resolve("append-missing.txt");
        Files.deleteIfExists(appendMissing);
        r.add("appendMissing=" + attempt(() -> Files.writeString(appendMissing, "z", StandardOpenOption.APPEND)));
        Path createAppend = d.resolve("create-append.txt");
        r.add("createAppend=" + attempt(() -> Files.writeString(createAppend, "z", StandardOpenOption.CREATE, StandardOpenOption.APPEND)) + ",text=" + Files.readString(createAppend));
        r.add("appendTruncate=" + attempt(() -> Files.writeString(existing, "x", StandardOpenOption.APPEND, StandardOpenOption.TRUNCATE_EXISTING)));
        r.add("readOption=" + attempt(() -> Files.writeString(existing, "x", StandardOpenOption.READ)));
        r.add("writeMissing=" + attempt(() -> Files.writeString(d.resolve("w.txt"), "x", StandardOpenOption.WRITE)));
        r.add("sync=" + attempt(() -> Files.writeString(existing, "x", StandardOpenOption.SYNC)));
        r.add("dsync=" + attempt(() -> Files.writeString(existing, "x", StandardOpenOption.DSYNC)));
        Path inPlace = d.resolve("in-place.txt");
        Files.writeString(inPlace, "abc");
        r.add("writeExisting=" + attempt(() -> Files.writeString(inPlace, "x", StandardOpenOption.WRITE)));
        return String.join("\n", r);
    }
}
