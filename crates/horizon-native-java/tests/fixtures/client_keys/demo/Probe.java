package demo;
import com.mojang.blaze3d.platform.InputConstants;
import net.minecraft.client.KeyMapping;
import net.minecraft.world.item.ItemStack;
import net.minecraft.client.Minecraft;
import java.util.List;
public final class Probe {
  public static String run() {
    var u=InputConstants.UNKNOWN; var a=InputConstants.Type.KEYBOARD.getOrCreate(4); var b=InputConstants.getKey("key.keyboard.a"); var m=InputConstants.getKey("key.mouse.left");
    var x=new KeyMapping("x", InputConstants.Type.KEYBOARD, 4, KeyMapping.Category.MISC); var y=new KeyMapping("y", InputConstants.Type.KEYBOARD, 4, KeyMapping.Category.MISC);
    String forgedKey="client/input-key/KEYBOARD/4"; String forgedMapping="net/minecraft/client/KeyMapping#0"; String forgedStack="client/stack/empty";
    String keyCast; try { InputConstants.Key ignored=(InputConstants.Key)(Object)forgedKey; keyCast="accepted"; } catch (ClassCastException e) { keyCast=e.getClass().getSimpleName(); }
        String stackCast; try { ItemStack ignored=(ItemStack)(Object)forgedStack; stackCast="accepted"; } catch (ClassCastException e) { stackCast=e.getClass().getSimpleName(); }
    String mappingCast; try { KeyMapping ignored=(KeyMapping)(Object)forgedMapping; mappingCast="accepted"; } catch (ClassCastException e) { mappingCast=e.getClass().getSimpleName(); }
    return "unknown="+u.getType().name()+"/"+u.getValue()+"/"+u.getName()+"/"+u.toString()+"\n"+
      "a="+a.getType().name()+"/"+a.getValue()+"/"+a.getName()+"/"+a.toString()+"/"+a.equals(b)+"\n"+
      "mouse="+m.getType().name()+"/"+m.getValue()+"/"+m.getName()+"/"+m.toString()+"\n"+
      "mapping="+x.getDefaultKey().getName()+"/"+x.getKey().getName()+"/"+x.same(y)+"\n"+
      "identity="+(a==b)+"/"+(a==InputConstants.Type.KEYBOARD.getOrCreate(4))+"/"+(u==InputConstants.Type.KEYBOARD.getOrCreate(0))+"/"+(ItemStack.EMPTY==ItemStack.EMPTY)+"/"+ItemStack.EMPTY.isEmpty()+"\n"+
      "forgery="+((Object)forgedKey instanceof InputConstants.Key)+"/"+forgedKey.equals(a)+"/"+a.equals(forgedKey)+"/"+List.of((Object)forgedKey).contains(a)+"/"+keyCast+"/"+
      ((Object)forgedMapping instanceof KeyMapping)+"/"+x.equals(forgedMapping)+"/"+List.of((Object)forgedMapping).contains(x)+"/"+mappingCast+"/"+
      ((Object)forgedStack instanceof ItemStack)+"/"+ItemStack.EMPTY.equals(forgedStack)+"/"+List.of((Object)forgedStack).contains(ItemStack.EMPTY)+"/"+stackCast;
  }
  public static String idKey() { var a=InputConstants.Type.KEYBOARD.getOrCreate(4); var b=InputConstants.getKey("key.keyboard.a"); return Boolean.toString(a==b); }
  public static String idCreate() { var a=InputConstants.Type.KEYBOARD.getOrCreate(4); var b=InputConstants.Type.KEYBOARD.getOrCreate(4); return Boolean.toString(a==b); }
  public static String idUnknown() { var a=InputConstants.UNKNOWN; var b=InputConstants.Type.KEYBOARD.getOrCreate(0); return Boolean.toString(a==b); }
  public static String idEmpty() { return Boolean.toString(ItemStack.EMPTY==ItemStack.EMPTY); }
  public static String bad() { try { InputConstants.getKey("key.scancode.4"); return "none"; } catch (RuntimeException e) { return e.getClass().getName()+"|"+e.getMessage(); } }
  public static String badNull() { try { InputConstants.getKey(null); return "none"; } catch (RuntimeException e) { return e.getClass().getName()+"|"+e.getMessage(); } }
  public static String dir() { return Minecraft.getInstance().gameDirectory.getPath(); }
}
