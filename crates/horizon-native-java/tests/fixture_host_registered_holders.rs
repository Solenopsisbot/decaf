//! Mod-created holders: the host answers only ids observed in registration calls.
//!
//! `java25.txt` is captured from the real 26.3 server classes after bootstrap;
//! the Rust fixture keeps only the signatures needed to exercise the same calls.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;

const CAPTURED: &str = include_str!("fixtures/registered_holders/java25.txt");

fn captured_rows() -> Vec<&'static str> {
    CAPTURED
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect()
}

const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        r#"package net.minecraft.resources;
public final class Identifier {
  public static Identifier fromNamespaceAndPath(String namespace, String path) { return null; }
  public static Identifier parse(String text) { return null; }
}
"#,
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        r#"package net.minecraft.resources;
public class ResourceKey<T> {
  public static <T> ResourceKey<T> create(ResourceKey<?> r, Identifier i) { return null; }
  public Identifier identifier() { return null; }
  public String toString() { return null; }
}
"#,
    ),
    (
        "net/minecraft/core/Holder.java",
        r#"package net.minecraft.core;
import java.util.Optional;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
public interface Holder<T> {
  interface Reference<T> extends Holder<T> {
    ResourceKey<T> key();
    static <T> Reference<T> createStandAlone(HolderOwner<T> owner, ResourceKey<T> key) { return null; }
    static <T> Reference<T> createIntrusive(HolderOwner<T> owner, T value) { return null; }
  }
  interface Direct<T> extends Holder<T> {}
  T value();
  boolean isBound();
  boolean areComponentsBound();
  boolean is(Identifier id);
  boolean is(ResourceKey<T> key);
  String getRegisteredName();
  default Optional<Identifier> getRegisteredNameIfPresent() { return unwrapKey().map(ResourceKey::identifier); }
  Optional<ResourceKey<T>> unwrapKey();
  Kind kind();
  enum Kind { REFERENCE, DIRECT }
  static <T> Holder<T> direct(T value) { return null; }
  static <T> Holder<T> direct(T value, net.minecraft.core.component.DataComponentMap map) { return null; }
}
"#,
    ),
    (
        "net/minecraft/core/HolderOwner.java",
        "package net.minecraft.core; public interface HolderOwner<T> {}\n",
    ),
    (
        "net/minecraft/core/component/DataComponentMap.java",
        "package net.minecraft.core.component; public interface DataComponentMap {}\n",
    ),
    (
        "net/minecraft/core/Registry.java",
        r#"package net.minecraft.core;
import java.util.Optional;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
public interface Registry<T> extends HolderOwner<T> {
  static <T> T register(Registry<? super T> registry, Identifier id, T value) { return value; }
  static <T> Holder.Reference<T> registerForHolder(Registry<? super T> registry, Identifier id, T value) { return null; }
  Holder<T> wrapAsHolder(T value);
  Optional<Holder.Reference<T>> get(Identifier id);
}
"#,
    ),
    (
        "net/minecraft/core/registries/Registries.java",
        "package net.minecraft.core.registries; import net.minecraft.resources.ResourceKey; public final class Registries { public static final ResourceKey<?> ATTRIBUTE = null; }\n",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        r#"package net.minecraft.core.registries;
import net.minecraft.core.Registry;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.entity.ai.attributes.Attribute;
public final class BuiltInRegistries {
  public static final Registry<MobEffect> MOB_EFFECT = null;
  public static final Registry<Attribute> ATTRIBUTE = null;
}
"#,
    ),
    (
        "net/minecraft/world/effect/MobEffectCategory.java",
        r#"package net.minecraft.world.effect;
public enum MobEffectCategory { BENEFICIAL, HARMFUL }
"#,
    ),
    (
        "net/minecraft/world/effect/MobEffect.java",
        r#"package net.minecraft.world.effect;
public class MobEffect { protected MobEffect(MobEffectCategory category, int color) {} }
"#,
    ),
    (
        "net/minecraft/world/entity/ai/attributes/Attribute.java",
        "package net.minecraft.world.entity.ai.attributes; public class Attribute {}\n",
    ),
    (
        "net/minecraft/world/entity/ai/attributes/RangedAttribute.java",
        r#"package net.minecraft.world.entity.ai.attributes;
public class RangedAttribute extends Attribute {
  public RangedAttribute(String id, double value, double min, double max) {}
}
"#,
    ),
    (
        "demo/Probe.java",
        r#"package demo;
import net.minecraft.core.Holder;
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
  static final class Effect extends MobEffect {
    Effect() { super(MobEffectCategory.BENEFICIAL, 1); }
  }
  static String facts(String label, Holder<?> holder, Identifier id, Object expected) {
    Object key = holder.unwrapKey().orElse(null);
    return label + "|" + holder.getRegisteredName() + "|" + key + "|"
      + (holder.value() == expected) + "|" + holder.isBound() + "|" + holder.kind()
      + "|" + holder.is(id);
  }
  public static String effect() {
    Identifier id = Identifier.fromNamespaceAndPath("probe", "effect");
    MobEffect value = new Effect();
    Holder.Reference<MobEffect> holder = Registry.registerForHolder(BuiltInRegistries.MOB_EFFECT, id, value);
    return facts("effect", holder, id, value);
  }
  static Attribute attributeValue(Identifier id) {
    return Registry.register(BuiltInRegistries.ATTRIBUTE, id,
      new RangedAttribute("attribute", 1.0, 0.0, 2.0));
  }
  public static String attribute() {
    Identifier id = Identifier.fromNamespaceAndPath("probe", "attribute");
    Attribute value = attributeValue(id);
    return facts("direct", Holder.direct(value), id, value);
  }
  public static String wrapped() {
    Identifier id = Identifier.fromNamespaceAndPath("probe", "attribute");
    Attribute value = attributeValue(id);
    return facts("wrapped", BuiltInRegistries.ATTRIBUTE.wrapAsHolder(value), id, value);
  }
  static String error(Throwable error) { return "THROW:" + error.getClass().getName() + ":" + error.getMessage(); }
  static String name(Holder<?> holder) { try { return "OK:" + holder.getRegisteredName(); } catch (Throwable e) { return error(e); } }
  static String present(Holder<?> holder) { try { return "OK:" + holder.getRegisteredNameIfPresent(); } catch (Throwable e) { return error(e); } }
  static String key(Holder.Reference<?> holder) { try { return "OK:" + holder.key(); } catch (Throwable e) { return error(e); } }
  static String unwrapKey(Holder<?> holder) { try { return "OK:" + holder.unwrapKey(); } catch (Throwable e) { return error(e); } }
  static String value(Holder<?> holder) { try { return "OK:" + holder.value(); } catch (Throwable e) { return error(e); } }
  static String bound(Holder<?> holder) { try { return "OK:" + holder.isBound(); } catch (Throwable e) { return error(e); } }
  static String text(Holder.Reference<?> holder) { try { return "OK:" + holder; } catch (Throwable e) { return error(e); } }
  static String referenceFacts(String label, Holder.Reference<?> holder) {
    return label + "|name=" + name(holder) + "|present=" + present(holder) + "|key=" + key(holder)
      + "|unwrapKey=" + unwrapKey(holder) + "|value=" + value(holder) + "|bound=" + bound(holder)
      + "|toString=" + text(holder);
  }
  public static String unbound() {
    ResourceKey<Attribute> key = ResourceKey.create(Registries.ATTRIBUTE, Identifier.fromNamespaceAndPath("probe", "unbound"));
    return referenceFacts("unbound", Holder.Reference.createStandAlone(null, key));
  }
  static final class Named { public String toString() { return "probe:named"; } }
  public static String intrusive() {
    return referenceFacts("intrusive", Holder.Reference.createIntrusive(null, "intrusive"));
  }
  public static String intrusiveRegistryOwner() {
    return referenceFacts("intrusiveRegistryOwner", Holder.Reference.createIntrusive((net.minecraft.core.HolderOwner) BuiltInRegistries.ATTRIBUTE, "intrusive"));
  }
  public static String intrusiveJarValue() {
    return "intrusiveJarValue|toString=" + text(Holder.Reference.createIntrusive(null, new Named()));
  }
}
"#,
    ),
];

fn fixture(test: &str) -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(test, STUBS)
}

#[test]
fn observed_registration_holders_match_java25_oracle() {
    let fixture = fixture("registered-holders");
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "effect"), captured_rows()[0]);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "attribute"), captured_rows()[1]);
    assert_eq!(fixture.text_on(&mut host, "wrapped"), captured_rows()[2]);
    assert_eq!(fixture.text_on(&mut host, "unbound"), captured_rows()[3]);
    assert_eq!(fixture.text_on(&mut host, "intrusive"), captured_rows()[4]);
    assert_eq!(
        fixture.text_on(&mut host, "intrusiveRegistryOwner"),
        captured_rows()[5]
    );
    assert_eq!(
        fixture.text_on(&mut host, "intrusiveJarValue"),
        captured_rows()[6]
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
