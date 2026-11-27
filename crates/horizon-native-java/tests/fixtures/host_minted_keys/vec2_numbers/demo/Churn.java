package demo;

/**
 * Garbage: objects with a field written, so the resident heap grows past its collection trigger
 * (1024 entries past the live set) and the next collection frees every number they took (#669).
 */
public final class Churn {
  static final class Cell {
    int value;
  }

  public static int churn(int count) {
    int sum = 0;
    for (int i = 0; i < count; i++) {
      Cell cell = new Cell();
      cell.value = 1;
      sum += cell.value;
    }
    return sum;
  }
}
