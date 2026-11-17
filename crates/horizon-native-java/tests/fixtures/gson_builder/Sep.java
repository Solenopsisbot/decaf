package demo;

import com.google.gson.FormattingStyle;
import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class Sep {
  public static String separatorRows() {
    Map<String, Object> object = new LinkedHashMap<>();
    object.put("a", 1);
    object.put("b", List.of(1, 2));
    Gson compact = new GsonBuilder().setFormattingStyle(
        FormattingStyle.COMPACT.withSpaceAfterSeparators(true)).create();
    Gson noNewline = new GsonBuilder().setFormattingStyle(
        FormattingStyle.PRETTY.withNewline("")).create();
    return compact.toJson(object) + "|" + compact.toJson(List.of(1, 2)) + "|"
        + compact.toJson(new LinkedHashMap<>()) + "|" + compact.toJson(List.of()) + "|"
        + noNewline.toJson(object) + "|" + noNewline.toJson(List.of(1, 2)) + "|"
        + noNewline.toJson(new LinkedHashMap<>()) + "|" + noNewline.toJson(List.of());
  }

  public static void main(String[] args) {
    System.out.print(separatorRows());
  }
}
