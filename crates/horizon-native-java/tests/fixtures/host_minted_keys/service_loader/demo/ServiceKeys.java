package demo;

import java.util.ServiceLoader;

/**
 * Every `ServiceLoader.load` is a new loader (#669): one made before a game hook and one made
 * after it are two objects, whatever numbers the heap gave them.
 */
public final class ServiceKeys {
  public interface Service {}

  public static Object load() {
    return ServiceLoader.load(Service.class);
  }

  public static String same(Object first) {
    Object second = ServiceLoader.load(Service.class);
    return "" + (first == second) + "|" + first.equals(second) + "|" + second;
  }

  public static void main(String[] args) {
    System.out.println(same(load()));
  }
}
