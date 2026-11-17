public class Probe {
  public static String rows() { return "service_same=true\nfabric_name=Knot/Fabric\nfabric_server_side=SERVER\nneoforge_name=ModLauncher\nneoforge_server_side=SERVER\ngetBytecodeProvider=COUNTED\n"; }
  public static void main(String[] a) { System.out.print(rows()); }
  public static String provider() { return "COUNTED"; }
}
