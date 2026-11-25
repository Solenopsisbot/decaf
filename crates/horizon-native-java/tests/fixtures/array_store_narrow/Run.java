import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Loads the compiled demo/Raw with its narrowing instructions patched out (the same byte edits as
 * fixture_array_store_narrow.rs), then prints each demo/Probe method's result. Usage: Run <classes dir>.
 */
public class Run {
    static int replace(byte[] bytes, int a, int b, int to) {
        int n = 0;
        for (int i = 0; i + 1 < bytes.length; i++) {
            if ((bytes[i] & 0xff) == a && (bytes[i + 1] & 0xff) == b) {
                bytes[i] = (byte) to;
                n++;
            }
        }
        return n;
    }

    static byte[] patchRaw(byte[] bytes) {
        // i2c castore, i2s sastore, i2b bastore (byte[] and boolean[]): the narrowing becomes a nop.
        check(replace(bytes, 0x92, 0x55, 0x00), 1);
        check(replace(bytes, 0x93, 0x56, 0x00), 1);
        check(replace(bytes, 0x91, 0x54, 0x00), 2);
        // boolStore: its newarray byte is the second one in the class file; change it to newarray boolean.
        int seen = 0;
        for (int i = 0; i + 1 < bytes.length; i++) {
            if ((bytes[i] & 0xff) == 0xbc && bytes[i + 1] == 8 && ++seen == 2) {
                bytes[i + 1] = 4;
            }
        }
        check(seen, 2);
        return bytes;
    }

    static void check(int got, int want) {
        if (got != want) throw new IllegalStateException("patch matched " + got + " times, wanted " + want);
    }

    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]);
        byte[] raw = patchRaw(Files.readAllBytes(dir.resolve("demo/Raw.class")));
        byte[] probe = Files.readAllBytes(dir.resolve("demo/Probe.class"));
        ClassLoader loader = new ClassLoader(Run.class.getClassLoader()) {
            @Override
            protected Class<?> findClass(String name) throws ClassNotFoundException {
                if (name.equals("demo.Raw")) return defineClass(name, raw, 0, raw.length);
                if (name.equals("demo.Probe")) return defineClass(name, probe, 0, probe.length);
                throw new ClassNotFoundException(name);
            }
        };
        Class<?> c = loader.loadClass("demo.Probe");
        for (String name : new String[] {"charStore", "shortStore", "byteStore", "boolStore"}) {
            Method m = c.getMethod(name);
            System.out.println(name + "=" + m.invoke(null));
        }
    }
}
