package demo;
import net.fabricmc.fabric.api.creativetab.v1.CreativeModeTabEvents;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.CreativeModeTab;
public final class Probe {
  static boolean fired;
  static ResourceKey<CreativeModeTab> key(String id) { return ResourceKey.create(ResourceKey.createRegistryKey(Identifier.parse("minecraft:creative_mode_tab")), Identifier.parse(id)); }
  public static String rows() {
    ResourceKey<CreativeModeTab> keyA = key("spell_engine:main");
    ResourceKey<CreativeModeTab> keyB = key("spell_engine:other");
    Event<CreativeModeTabEvents.ModifyOutput> first = CreativeModeTabEvents.modifyOutputEvent(keyA);
    Event<CreativeModeTabEvents.ModifyOutput> second = CreativeModeTabEvents.modifyOutputEvent(keyA);
    Event<CreativeModeTabEvents.ModifyOutput> other = CreativeModeTabEvents.modifyOutputEvent(keyB);
    String eventClass = first.getClass().getName();
    String same = Boolean.toString(first == second);
    String otherSame = Boolean.toString(first == other);
    String registerReturn = "void";
    first.register(output -> fired = true);
    return "event=" + eventClass + "\nsame_event=" + same + "\nother_key_same=" + otherSame + "\nregister_return=" + registerReturn + "\nlistener_fired=" + fired + "\n";
  }
  public static void main(String[] args) { System.out.print(rows()); }
}
