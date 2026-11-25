import java.lang.reflect.Method;
import java.util.Arrays;

public class Run {
    public static void main(String[] args) throws Exception {
        Class<?> probe = Class.forName("demo.Probe");
        String[] names = {
            "charCast", "byteCast", "shortCast", "charCastInline", "charFromWide", "chained", "charArith",
            "byteShortArith", "charFields", "charArrays", "narrowArrays", "charStrings", "charCalls",
        };
        for (String name : names) {
            Method m = probe.getMethod(name);
            System.out.println(name + "=" + m.invoke(null));
        }
    }
}
