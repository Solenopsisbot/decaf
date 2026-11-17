package demo;
import net.minecraft.world.phys.Vec2;
public final class Probe {
  static void p(StringBuilder o,String n,Vec2 v){o.append(n).append('|').append(v.x).append('|').append(v.y).append('|').append(Float.floatToRawIntBits(v.x)).append('|').append(Float.floatToRawIntBits(v.y)).append('\n');}
  public static String rows(){StringBuilder o=new StringBuilder();p(o,"ZERO",Vec2.ZERO);p(o,"ONE",Vec2.ONE);p(o,"UNIT_X",Vec2.UNIT_X);p(o,"NEG_UNIT_X",Vec2.NEG_UNIT_X);p(o,"UNIT_Y",Vec2.UNIT_Y);p(o,"NEG_UNIT_Y",Vec2.NEG_UNIT_Y);p(o,"MAX",Vec2.MAX);p(o,"MIN",Vec2.MIN);o.append("ZERO_EQ_ZERO|").append(Vec2.ZERO==Vec2.ZERO).append('\n');o.append("ZERO_NE_ONE|").append(Vec2.ZERO!=Vec2.ONE).append('\n');o.append("NEW_NE_ZERO|").append(new Vec2(0f,0f)!=Vec2.ZERO).append('\n');return o.toString();}
}
