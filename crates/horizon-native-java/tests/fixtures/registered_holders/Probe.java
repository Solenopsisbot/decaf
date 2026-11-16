package demo;

import com.mojang.serialization.Lifecycle;
import net.minecraft.SharedConstants;
import net.minecraft.core.Holder;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectCategory;
import net.minecraft.world.entity.ai.attributes.Attribute;
import net.minecraft.world.entity.ai.attributes.RangedAttribute;

public final class Probe {
    static final class Named { @Override public String toString() { return "probe:named"; } }
    static final class Effect extends MobEffect { Effect() { super(MobEffectCategory.BENEFICIAL, 1); } }
    static String facts(String label, Holder<?> holder, Identifier id, Object expected) {
        Object key = holder.unwrapKey().orElse(null);
        return label + "|" + holder.getRegisteredName() + "|" + key + "|" + (holder.value() == expected) + "|" + holder.isBound() + "|" + holder.kind() + "|" + holder.is(id);
    }
    static String error(Throwable error) { return "THROW:" + error.getClass().getName() + ":" + error.getMessage(); }
    static String name(Holder<?> holder) { try { return "OK:" + holder.getRegisteredName(); } catch (Throwable e) { return error(e); } }
    static String present(Holder<?> holder) { try { return "OK:" + holder.getRegisteredNameIfPresent(); } catch (Throwable e) { return error(e); } }
    static String key(Holder.Reference<?> holder) { try { return "OK:" + holder.key(); } catch (Throwable e) { return error(e); } }
    static String unwrapKey(Holder<?> holder) { try { return "OK:" + holder.unwrapKey(); } catch (Throwable e) { return error(e); } }
    static String value(Holder<?> holder) { try { return "OK:" + holder.value(); } catch (Throwable e) { return error(e); } }
    static String bound(Holder<?> holder) { try { return "OK:" + holder.isBound(); } catch (Throwable e) { return error(e); } }
    static String text(Holder.Reference<?> holder) { try { return "OK:" + holder; } catch (Throwable e) { return error(e); } }
    static String referenceFacts(String label, Holder.Reference<?> holder) { return label + "|name=" + name(holder) + "|present=" + present(holder) + "|key=" + key(holder) + "|unwrapKey=" + unwrapKey(holder) + "|value=" + value(holder) + "|bound=" + bound(holder) + "|toString=" + text(holder); }
    public static String effect() {
        Identifier id = Identifier.fromNamespaceAndPath("probe", "effect"); MobEffect value = new Effect();
        Registry<MobEffect> registry = new MappedRegistry<>(Registries.MOB_EFFECT, Lifecycle.stable());
        Holder.Reference<MobEffect> holder = Registry.registerForHolder(registry, id, value); registry.freeze();
        return facts("effect", holder, id, value);
    }
    static Attribute attributeValue(Identifier id) { Registry<Attribute> registry = new MappedRegistry<>(Registries.ATTRIBUTE, Lifecycle.stable()); Attribute value = Registry.register(registry, id, new RangedAttribute("attribute", 1.0, 0.0, 2.0)); registry.freeze(); return value; }
    public static String attribute() { Identifier id = Identifier.fromNamespaceAndPath("probe", "attribute"); Attribute value = attributeValue(id); return facts("direct", Holder.direct(value), id, value); }
    public static String wrapped() { Identifier id = Identifier.fromNamespaceAndPath("probe", "attribute"); Registry<Attribute> registry = new MappedRegistry<>(Registries.ATTRIBUTE, Lifecycle.stable()); Attribute value = Registry.register(registry, id, new RangedAttribute("attribute", 1.0, 0.0, 2.0)); registry.freeze(); return facts("wrapped", registry.wrapAsHolder(value), id, value); }
    public static String unbound() { ResourceKey<Attribute> key = ResourceKey.create(Registries.ATTRIBUTE, Identifier.fromNamespaceAndPath("probe", "unbound")); return referenceFacts("unbound", Holder.Reference.createStandAlone(null, key)); }
    public static String intrusive() { return referenceFacts("intrusive", Holder.Reference.createIntrusive(null, "intrusive")); }
    public static String intrusiveRegistryOwner() { return referenceFacts("intrusiveRegistryOwner", Holder.Reference.createIntrusive((net.minecraft.core.HolderOwner) BuiltInRegistries.ATTRIBUTE, "intrusive")); }
    public static String intrusiveJarValue() { return "intrusiveJarValue|toString=" + text(Holder.Reference.createIntrusive(null, new Named())); }
    public static void main(String[] args) { SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap(); System.out.println(effect()); System.out.println(attribute()); System.out.println(wrapped()); System.out.println(unbound()); System.out.println(intrusive()); System.out.println(intrusiveRegistryOwner()); System.out.println(intrusiveJarValue()); }
}
