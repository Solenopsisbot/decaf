package demo;
import net.minecraft.server.packs.*; import net.minecraft.server.packs.resources.*; import net.minecraft.resources.*;
import net.fabricmc.fabric.api.resource.*; import net.fabricmc.fabric.api.resource.v1.*; import net.fabricmc.fabric.api.resource.v1.reloader.*;
public final class Probe {
  public static void main(String[] args){System.out.println(fabric());System.out.println(v1());System.out.println(converter());}
  static IdentifiableResourceReloadListener listener(String id) { return new IdentifiableResourceReloadListener(){ public Identifier getFabricId(){return Identifier.parse(id);} public java.util.Collection<Identifier> getFabricDependencies(){return java.util.List.of(Identifier.parse("demo:dep"));} }; }
  public static String fabric() {
    ResourceManagerHelper a=ResourceManagerHelper.get(PackType.CLIENT_RESOURCES), b=ResourceManagerHelper.get(PackType.CLIENT_RESOURCES);
    a.registerReloadListener(listener("demo:one")); a.registerReloadListener(listener("demo:two"));
    String dup; try { a.registerReloadListener(listener("demo:one")); dup="none"; } catch (IllegalStateException e) { dup="duplicate"; }
    return (a==b)+"|"+dup;
  }
  public static String v1() {
    ResourceLoader a=ResourceLoader.get(PackType.CLIENT_RESOURCES), b=ResourceLoader.get(PackType.CLIENT_RESOURCES);
    a.registerReloadListener(Identifier.parse("demo:one"), new PreparableReloadListener(){});
    a.addListenerOrdering(Identifier.parse("demo:one"), ResourceReloaderKeys.Client.ATLAS);
    return (a==b)+"|"+ResourceReloaderKeys.Client.ATLAS;
  }
  public static String converter() {
    FileToIdConverter c=FileToIdConverter.json("textures"); Identifier id=Identifier.parse("demo:foo/bar"); Identifier file=c.idToFile(id); Identifier back=c.fileToId(file);
    return file+"|"+back+"|"+c.matches(file)+"|"+c.prefix()+"|"+c.extension();
  }
  public static String builtin() { return ""+ResourceLoader.registerBuiltinPack(Identifier.parse("demo:pack"), null, null); }
  public static String malformed() { try { FileToIdConverter.json("textures").fileToId(Identifier.parse("demo:x")); return "none"; } catch (StringIndexOutOfBoundsException e) { return e.getMessage(); } }
}
