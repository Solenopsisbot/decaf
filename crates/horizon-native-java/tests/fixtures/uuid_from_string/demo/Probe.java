package demo;
import java.util.UUID;
/**
 * Java 25's {@code UUID.fromString} on valid, legacy and invalid text: the UUID and its halves, or
 * the exception's class and message. Past the dash check only ASCII fields (the runtime counts a
 * non-ASCII one; see fixture_uuid_from_string.rs), and no null.
 */
public class Probe {
  static final String[] INPUTS = {
    "123e4567-e89b-12d3-a456-426614174000",
    "123E4567-E89B-12D3-A456-426614174000",
    "",
    "-",
    "----",
    "1-2-3-4-5",
    "0-0-0-0-0",
    "1-2-3-4",
    "1-2-3-4-5-6",
    "-1-2-3-4",
    "1--2-3-4",
    "1-2-3-4-",
    "g-2-3-4-5",
    "1-2-3-4-g",
    "+1-2-3-4-5",
    "1-2-3-4-+5",
    "1-2-3-4--5",
    " 123e4567-e89b-12d3-a456-426614174000",
    "123e4567-e89b-12d3-a456-426614174000 ",
    "123e4567-e89b-12d3-a456-4266141740000",
    "123e4567-e89b-12d3-a456-42661417400",
    "123e4567e89b12d3a456426614174000",
    "123e4567-e89b-12d3-a456_426614174000",
    "123e4567-e89b-12d3-a4560426614174000",
    "123e4567-e89b-12d3-a456-42661417400g",
    "123e4567-e89b-12d3-a456-4266141740-0",
    "123e45670e89b-12d3-a456-426614174000",
    "ffffffffffffffff-1-1-1-1",
    "10000000000000000-1-1-1-1",
    "7fffffffffffffff-1-1-1-1",
    "8000000000000000-1-1-1-1",
    "1-2-3-4-10000000000000000",
    "1-2-3-4-123456789abcdef",
    "123456789-12345-12345-12345-1234567890123",
    "1234567890123456789012345678901234567",
    "12345678901234567890123456789012345678901",
    "0x1-2-3-4-5",
    "1-2-3-4-5 ",
    " 1-2-3-4-5",
    "1 -2-3-4-5",
    "g-h-3-4-5",
    "1-h-3-4-g",
    "1--3-4-g",
    "-g-3-4-5",
    "+-2-3-4-5",
    "++1-2-3-4-5",
    "1-2-3-4-+",
    "+ffffffffffffffff-1-1-1-1",
    "+7fffffffffffffff-1-1-1-1",
    "\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9",
    "fffffffff-ffff-ffff-ffff-ffffffffffff",
    "1-12345-1-1-1",
    "1-2-3-4-5-",
    "a-b-c-d-e",
    "A-B-C-D-E",
    "00000000000000001-2-3-4-5",
    "0000000000000000000000000000000001-2",
    "\u00e9",
    "\u00e9-\u00e9",
    "1-2-3-4-5/",
    "/1-2-3-4-5",
  };
  static String row(String in) {
    try {
      UUID u = UUID.fromString(in);
      return "ok " + u + " " + u.getMostSignificantBits() + " " + u.getLeastSignificantBits();
    } catch (IllegalArgumentException e) {
      return e.getClass().getName() + ": " + e.getMessage();
    }
  }
  public static String rows() {
    StringBuilder s = new StringBuilder();
    for (String in : INPUTS) s.append('[').append(in).append("] ").append(row(in)).append('\n');
    return s.toString();
  }
  public static void main(String[] args) { System.out.print(rows()); }
}
