package demo;

import net.minecraft.world.damagesource.DamageTypes;

/** Every DamageTypes key constant, then one identity row. */
public final class Probe {
  public static String rows() {
    StringBuilder b = new StringBuilder();
    b.append("IN_FIRE|").append(DamageTypes.IN_FIRE).append('\n');
    b.append("CAMPFIRE|").append(DamageTypes.CAMPFIRE).append('\n');
    b.append("LIGHTNING_BOLT|").append(DamageTypes.LIGHTNING_BOLT).append('\n');
    b.append("ON_FIRE|").append(DamageTypes.ON_FIRE).append('\n');
    b.append("LAVA|").append(DamageTypes.LAVA).append('\n');
    b.append("HOT_FLOOR|").append(DamageTypes.HOT_FLOOR).append('\n');
    b.append("SULFUR_CUBE_HOT|").append(DamageTypes.SULFUR_CUBE_HOT).append('\n');
    b.append("IN_WALL|").append(DamageTypes.IN_WALL).append('\n');
    b.append("CRAMMING|").append(DamageTypes.CRAMMING).append('\n');
    b.append("DROWN|").append(DamageTypes.DROWN).append('\n');
    b.append("STARVE|").append(DamageTypes.STARVE).append('\n');
    b.append("CACTUS|").append(DamageTypes.CACTUS).append('\n');
    b.append("FALL|").append(DamageTypes.FALL).append('\n');
    b.append("ENDER_PEARL|").append(DamageTypes.ENDER_PEARL).append('\n');
    b.append("FLY_INTO_WALL|").append(DamageTypes.FLY_INTO_WALL).append('\n');
    b.append("FELL_OUT_OF_WORLD|").append(DamageTypes.FELL_OUT_OF_WORLD).append('\n');
    b.append("GENERIC|").append(DamageTypes.GENERIC).append('\n');
    b.append("MAGIC|").append(DamageTypes.MAGIC).append('\n');
    b.append("WITHER|").append(DamageTypes.WITHER).append('\n');
    b.append("DRAGON_BREATH|").append(DamageTypes.DRAGON_BREATH).append('\n');
    b.append("DRY_OUT|").append(DamageTypes.DRY_OUT).append('\n');
    b.append("SWEET_BERRY_BUSH|").append(DamageTypes.SWEET_BERRY_BUSH).append('\n');
    b.append("FREEZE|").append(DamageTypes.FREEZE).append('\n');
    b.append("STALAGMITE|").append(DamageTypes.STALAGMITE).append('\n');
    b.append("FALLING_BLOCK|").append(DamageTypes.FALLING_BLOCK).append('\n');
    b.append("FALLING_ANVIL|").append(DamageTypes.FALLING_ANVIL).append('\n');
    b.append("FALLING_STALACTITE|").append(DamageTypes.FALLING_STALACTITE).append('\n');
    b.append("STING|").append(DamageTypes.STING).append('\n');
    b.append("MOB_ATTACK|").append(DamageTypes.MOB_ATTACK).append('\n');
    b.append("MOB_ATTACK_NO_AGGRO|").append(DamageTypes.MOB_ATTACK_NO_AGGRO).append('\n');
    b.append("PLAYER_ATTACK|").append(DamageTypes.PLAYER_ATTACK).append('\n');
    b.append("SPEAR|").append(DamageTypes.SPEAR).append('\n');
    b.append("ARROW|").append(DamageTypes.ARROW).append('\n');
    b.append("TRIDENT|").append(DamageTypes.TRIDENT).append('\n');
    b.append("MOB_PROJECTILE|").append(DamageTypes.MOB_PROJECTILE).append('\n');
    b.append("SPIT|").append(DamageTypes.SPIT).append('\n');
    b.append("WIND_CHARGE|").append(DamageTypes.WIND_CHARGE).append('\n');
    b.append("FIREWORKS|").append(DamageTypes.FIREWORKS).append('\n');
    b.append("FIREBALL|").append(DamageTypes.FIREBALL).append('\n');
    b.append("UNATTRIBUTED_FIREBALL|").append(DamageTypes.UNATTRIBUTED_FIREBALL).append('\n');
    b.append("WITHER_SKULL|").append(DamageTypes.WITHER_SKULL).append('\n');
    b.append("THROWN|").append(DamageTypes.THROWN).append('\n');
    b.append("INDIRECT_MAGIC|").append(DamageTypes.INDIRECT_MAGIC).append('\n');
    b.append("THORNS|").append(DamageTypes.THORNS).append('\n');
    b.append("EXPLOSION|").append(DamageTypes.EXPLOSION).append('\n');
    b.append("PLAYER_EXPLOSION|").append(DamageTypes.PLAYER_EXPLOSION).append('\n');
    b.append("SONIC_BOOM|").append(DamageTypes.SONIC_BOOM).append('\n');
    b.append("BAD_RESPAWN_POINT|").append(DamageTypes.BAD_RESPAWN_POINT).append('\n');
    b.append("OUTSIDE_BORDER|").append(DamageTypes.OUTSIDE_BORDER).append('\n');
    b.append("GENERIC_KILL|").append(DamageTypes.GENERIC_KILL).append('\n');
    b.append("MACE_SMASH|").append(DamageTypes.MACE_SMASH).append('\n');
    b.append("same|").append(DamageTypes.PLAYER_ATTACK == DamageTypes.PLAYER_ATTACK).append('\n');
    return b.toString();
  }

  public static void main(String[] args) {
    System.out.print(rows());
  }
}
