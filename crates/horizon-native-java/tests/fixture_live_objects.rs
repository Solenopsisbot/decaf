//! Live game objects (`HostKind::LiveGameObject`): a Sunrise entity or menu the host hands a jar.
//!
//! No JVM can run this case (the objects live in the running game), so the oracle is split:
//! * the class rows come from javap: `fixtures/live_objects/javap_facts.txt` holds the 26.3 jar's
//!   own declaration lines (command in the README), and the test follows their `extends` chain to
//!   derive what `instanceof` must answer;
//! * removal and identity rows are Decaf's host contract, asserted inline;
//! * every call the host can't answer must be counted: each row asserts exactly which calls it
//!   added to `unknown_host_calls`, so a default value is never mistaken for an answer.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{HostRef, StdHost, Value};
use javac_fixture::Fixture;
use std::collections::BTreeMap;

const JAVAP: &str = include_str!("fixtures/live_objects/javap_facts.txt");

const PROBE: &str = r#"package demo;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.inventory.AbstractContainerMenu;
public final class Probe {
  public static boolean isPlayer(ServerPlayer p) { return p instanceof net.minecraft.world.entity.player.Player; }
  public static boolean isAvatar(ServerPlayer p) { return p instanceof net.minecraft.world.entity.Avatar; }
  public static boolean isLiving(ServerPlayer p) { return p instanceof net.minecraft.world.entity.LivingEntity; }
  public static boolean isEntity(ServerPlayer p) { return p instanceof net.minecraft.world.entity.Entity; }
  public static boolean menuIsEntity(AbstractContainerMenu m) { return ((Object) m) instanceof net.minecraft.world.entity.Entity; }
  public static boolean same(ServerPlayer p) { return p == p; }
  public static boolean removed(ServerPlayer p) { return p.isRemoved(); }
  public static int hash(ServerPlayer p) { return p.hashCode(); }
  public static Object cls(ServerPlayer p) { return p.getClass(); }
  public static double x(ServerPlayer p) { return p.getX(); }
  public static void write(ServerPlayer p) { p.setPos(1, 2, 3); }
}
"#;

/// Compile-only stubs: `Fixture::build` ships only `demo/`, so every call on a live handle reaches
/// the host. Their shape follows `javap_facts.txt`.
const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/world/entity/Entity.java",
        "package net.minecraft.world.entity; public abstract class Entity { public final boolean isRemoved(){return false;} public final double getX(){return 0;} public void setPos(double a,double b,double c){} }",
    ),
    (
        "net/minecraft/world/entity/LivingEntity.java",
        "package net.minecraft.world.entity; public abstract class LivingEntity extends Entity {}",
    ),
    (
        "net/minecraft/world/entity/Avatar.java",
        "package net.minecraft.world.entity; public abstract class Avatar extends LivingEntity {}",
    ),
    (
        "net/minecraft/world/entity/player/Player.java",
        "package net.minecraft.world.entity.player; public abstract class Player extends net.minecraft.world.entity.Avatar {}",
    ),
    (
        "net/minecraft/server/level/ServerPlayer.java",
        "package net.minecraft.server.level; public class ServerPlayer extends net.minecraft.world.entity.player.Player {}",
    ),
    (
        "net/minecraft/world/inventory/AbstractContainerMenu.java",
        "package net.minecraft.world.inventory; public abstract class AbstractContainerMenu {}",
    ),
];

const SP: &str = "(Lnet/minecraft/server/level/ServerPlayer;)";
const MENU: &str = "(Lnet/minecraft/world/inventory/AbstractContainerMenu;)";

/// `class -> superclass` from javap's declaration lines (`… class A extends B …`).
fn superclasses() -> BTreeMap<String, String> {
    JAVAP
        .lines()
        .filter_map(|line| {
            let rest = line.split(" class ").nth(1)?;
            let mut words = rest.split_whitespace();
            let class = words.next()?;
            if words.next()? != "extends" {
                return None;
            }
            Some((class.to_owned(), words.next()?.to_owned()))
        })
        .collect()
}

/// Whether javap's chain makes `class` a subclass of `wanted`.
fn javap_says_subclass(class: &str, wanted: &str) -> bool {
    let sup = superclasses();
    let mut at = class.to_owned();
    while let Some(next) = sup.get(&at) {
        if next == wanted {
            return true;
        }
        at = next.clone();
    }
    false
}

struct Run {
    fixture: Fixture,
    host: StdHost,
}

impl Run {
    /// Runs `Probe.<name>` on `arg`; returns its value and exactly the counted calls it added.
    fn call(&mut self, name: &str, desc: &str, arg: &Value) -> (Value, Vec<(String, usize)>) {
        let before = self.host.unknown_host_calls();
        let value = self
            .fixture
            .jar
            .invoke_method_with_args(
                "demo/Probe",
                name,
                desc,
                None,
                std::slice::from_ref(arg),
                &mut self.host,
            )
            .unwrap();
        let added = self
            .host
            .unknown_host_calls()
            .into_iter()
            .filter(|call| !before.contains(call))
            .collect();
        (value, added)
    }
}

fn counted(call: &str) -> Vec<(String, usize)> {
    vec![(call.to_owned(), 1)]
}

#[test]
fn live_object_rows() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let mut run = Run {
        fixture: Fixture::build("live-objects", &sources),
        host: StdHost::new(),
    };
    // The host hands these two out (the dispatcher does this when it passes them to a callback).
    run.host.live_object_issued("ServerPlayer:1");
    run.host.live_object_issued("AbstractContainerMenu:1");
    let player = Value::Host(HostRef::live_game_object_for_test("ServerPlayer:1"));
    let menu = Value::Host(HostRef::live_game_object_for_test(
        "AbstractContainerMenu:1",
    ));

    // Class rows: what javap's chain says, answered without counting.
    let server_player = "net.minecraft.server.level.ServerPlayer";
    for (method, class) in [
        ("isPlayer", "net.minecraft.world.entity.player.Player"),
        ("isAvatar", "net.minecraft.world.entity.Avatar"),
        ("isLiving", "net.minecraft.world.entity.LivingEntity"),
        ("isEntity", "net.minecraft.world.entity.Entity"),
    ] {
        assert!(
            javap_says_subclass(server_player, class),
            "javap chain lacks {class}"
        );
        assert_eq!(
            run.call(method, &format!("{SP}Z"), &player),
            (Value::Int(1), vec![]),
            "{method}"
        );
    }
    // A menu is not provably not an entity from what the host knows of it (its runtime class may
    // be any subclass), so the check is counted rather than answered.
    let (_, added) = run.call("menuIsEntity", &format!("{MENU}Z"), &menu);
    assert_eq!(
        added,
        counted("net/minecraft/world/entity/Entity#<instanceof>(Ljava/lang/Object;)Z")
    );

    // Host contract: identity, and removal answered from the host's own record.
    assert_eq!(
        run.call("same", &format!("{SP}Z"), &player),
        (Value::Int(1), vec![])
    );
    assert_eq!(
        run.call("removed", &format!("{SP}Z"), &player),
        (Value::Int(0), vec![]),
        "an issued, live player is not removed"
    );

    // Not modelled: counted, whatever default value comes back.
    for (method, desc, call) in [
        ("hash", "I", "java/lang/Object#hashCode()I"),
        (
            "cls",
            "Ljava/lang/Object;",
            "java/lang/Object#getClass()Ljava/lang/Class;",
        ),
        ("x", "D", "net/minecraft/server/level/ServerPlayer#getX()D"),
        (
            "write",
            "V",
            "net/minecraft/server/level/ServerPlayer#setPos(DDD)V",
        ),
    ] {
        let (_, added) = run.call(method, &format!("{SP}{desc}"), &player);
        assert_eq!(added, counted(call), "{method}");
    }

    // Removed while the jar still holds the handle: the same handle now answers true.
    run.host.live_object_removed("ServerPlayer:1", "KILLED");
    assert_eq!(
        run.call("removed", &format!("{SP}Z"), &player),
        (Value::Int(1), vec![])
    );

    // A key the host never issued: counted, never answered.
    let forged = Value::Host(HostRef::live_game_object_for_test("ServerPlayer:forged"));
    let (_, added) = run.call("removed", &format!("{SP}Z"), &forged);
    assert_eq!(
        added,
        counted("net/minecraft/server/level/ServerPlayer#isRemoved()Z")
    );
}
