package demo;
import net.minecraft.resources.Identifier;
import net.minecraft.core.particles.*;
import net.fabricmc.fabric.api.particle.v1.FabricParticleTypes;
import net.fabricmc.fabric.api.client.particle.v1.ParticleProviderRegistry;
import net.minecraft.client.gui.components.debug.*;
import net.minecraft.client.gui.screens.MenuScreens;
import net.fabricmc.fabric.api.client.rendering.v1.ModelLayerRegistry;
import net.fabricmc.fabric.api.client.rendering.v1.hud.VanillaHudElements;
import net.fabricmc.fabric.api.client.model.loading.v1.PreparableModelLoadingPlugin;
import net.minecraft.client.gui.components.debug.DebugScreenEntryStatus;

public class Probe {
  public static String identities() {
    ParticleType<?> a = FabricParticleTypes.simple();
    ParticleType<?> b = FabricParticleTypes.simple();
    ParticleProviderRegistry r1 = ParticleProviderRegistry.getInstance();
    ParticleProviderRegistry r2 = ParticleProviderRegistry.getInstance();
    r1.register(a, null);
    return (a == b) + "|" + (r1 == r2) + "|" + (DebugScreenEntries.FPS == DebugScreenEntries.FPS)
        + "|" + (DebugScreenProfile.DEFAULT == DebugScreenProfile.DEFAULT);
  }
  public static String duplicates() {
    try {
      net.minecraft.client.model.geom.ModelLayerLocation loc = new net.minecraft.client.model.geom.ModelLayerLocation(Identifier.withDefaultNamespace("x"), "main");
      ModelLayerRegistry.registerModelLayer(loc, new Object());
      ModelLayerRegistry.registerModelLayer(loc, new Object());
      return "none";
    } catch (IllegalArgumentException e) { return e.getClass().getSimpleName(); }
  }
  public static String particle_semantics() {
    SimpleParticleType a = FabricParticleTypes.simple(true);
    SimpleParticleType b = FabricParticleTypes.simple(false);
    ParticleType<?> c = FabricParticleTypes.complex(true, null, null);
    ParticleType<?> d = FabricParticleTypes.complex(false, null, null);
    return a.getOverrideLimiter() + "|" + b.getOverrideLimiter() + "|" + c.getOverrideLimiter() + "|" + d.getOverrideLimiter()
        + "|" + (a.getType() == a) + "|" + (a == b) + "|" + (c == d);
  }
  public static String particle_codecs() {
    SimpleParticleType a = FabricParticleTypes.simple(true);
    com.mojang.serialization.MapCodec<?> map = a.codec();
    net.minecraft.network.codec.StreamCodec<?, ?> stream = a.streamCodec();
    com.mojang.serialization.MapCodec<Object> suppliedMap = new com.mojang.serialization.MapCodec<>();
    net.minecraft.network.codec.StreamCodec<Object, Object> suppliedStream = new net.minecraft.network.codec.StreamCodec<>();
    ParticleType<?> complex = FabricParticleTypes.complex(true, suppliedMap, suppliedStream);
    return (map == a.codec()) + "|" + (stream == a.streamCodec()) + "|"
        + (complex.codec() == suppliedMap) + "|" + (complex.streamCodec() == suppliedStream);
  }
  public static String hud_ids() {
    return VanillaHudElements.CHAT.toString() + "|" + VanillaHudElements.INFO_BAR_FOREGROUND.toString();
  }
  public static String profiles() {
    java.util.Map<DebugScreenProfile, java.util.Map<Identifier, DebugScreenEntryStatus>> profiles = DebugScreenEntries.PROFILES;
    return (profiles.get(DebugScreenProfile.DEFAULT).get(DebugScreenEntries.FPS) == DebugScreenEntryStatus.IN_OVERLAY) + "|"
        + (profiles.get(DebugScreenProfile.PERFORMANCE).get(DebugScreenEntries.FPS) == DebugScreenEntryStatus.ALWAYS_ON) + "|"
        + (profiles.get(DebugScreenProfile.DEFAULT).getOrDefault(DebugScreenEntries.FPS, DebugScreenEntryStatus.ALWAYS_ON) == DebugScreenEntryStatus.IN_OVERLAY) + "|"
        + (profiles.get(DebugScreenProfile.PERFORMANCE).getOrDefault(DebugScreenEntries.FPS, DebugScreenEntryStatus.IN_OVERLAY) == DebugScreenEntryStatus.ALWAYS_ON);
  }
  public static String model_equal() {
    try {
      ModelLayerRegistry.registerModelLayer(new net.minecraft.client.model.geom.ModelLayerLocation(Identifier.withDefaultNamespace("x"), "main"), new Object());
      ModelLayerRegistry.registerModelLayer(new net.minecraft.client.model.geom.ModelLayerLocation(Identifier.withDefaultNamespace("x"), "main"), new Object());
      return "none";
    } catch (IllegalArgumentException e) { return e.getMessage(); }
  }
  public static String preparable_nulls() {
    String a;
    String b;
    try { PreparableModelLoadingPlugin.register(null, null); a = "none"; }
    catch (NullPointerException e) { a = e.getMessage(); }
    try { PreparableModelLoadingPlugin.register(new PreparableModelLoadingPlugin.DataLoader<Object>() {}, null); b = "none"; }
    catch (NullPointerException e) { b = e.getMessage(); }
    return a + "|" + b;
  }
  public static String fields() {
    return (DebugScreenEntries.FPS != null) + "|" + (net.fabricmc.fabric.api.client.rendering.v1.hud.VanillaHudElements.CHAT != null);
  }
}
