#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{HostKind, HostRef, StdHost, Value};
use javac_fixture::Fixture;
const PROBE: &str = r#"package demo; public final class Probe { public static String rows(net.minecraft.server.level.ServerPlayer p, net.minecraft.world.inventory.AbstractContainerMenu m){ StringBuilder s=new StringBuilder(); s.append(p instanceof net.minecraft.world.entity.player.Player).append('|').append(p instanceof net.minecraft.world.entity.LivingEntity).append('|').append(p instanceof net.minecraft.world.entity.Entity).append('|').append(p==p).append('|').append(p.isRemoved()); try{s.append('|').append(p.hashCode());}catch(Throwable e){s.append("|unknown_host_call");} try{s.append('|').append(p.getClass());}catch(Throwable e){s.append("|unknown_host_call");} try{s.append('|').append(p.getX());}catch(Throwable e){s.append("|unknown_host_call");} try{p.setPos(1,2,3);}catch(Throwable e){s.append("|unknown_host_call");} return s.toString(); }}"#;
#[test]
fn live_object_rows() {
    let f = Fixture::build_every_package(
        "live-objects",
        &[
            ("demo/Probe.java", PROBE),
            (
                "net/minecraft/world/entity/Entity.java",
                "package net.minecraft.world.entity; public class Entity { public boolean isRemoved(){throw new AssertionError();} public double getX(){throw new AssertionError();} public void setPos(double x,double y,double z){throw new AssertionError();} }",
            ),
            (
                "net/minecraft/world/entity/LivingEntity.java",
                "package net.minecraft.world.entity; public class LivingEntity extends Entity {}",
            ),
            (
                "net/minecraft/world/entity/player/Player.java",
                "package net.minecraft.world.entity.player; public class Player extends net.minecraft.world.entity.LivingEntity {}",
            ),
            (
                "net/minecraft/server/level/ServerPlayer.java",
                "package net.minecraft.server.level; public class ServerPlayer extends net.minecraft.world.entity.player.Player {}",
            ),
            (
                "net/minecraft/world/inventory/AbstractContainerMenu.java",
                "package net.minecraft.world.inventory; public abstract class AbstractContainerMenu {}",
            ),
        ],
    );
    let mut h = StdHost::new();
    let p = Value::Host(HostRef::new(HostKind::LiveGameObject, "ServerPlayer:1"));
    let m = Value::Host(HostRef::new(
        HostKind::LiveGameObject,
        "AbstractContainerMenu:1",
    ));
    let out=f.jar.invoke_method_with_args("demo/Probe","rows","(Lnet/minecraft/server/level/ServerPlayer;Lnet/minecraft/world/inventory/AbstractContainerMenu;)Ljava/lang/String;",None,&[p,m],&mut h).unwrap();
    assert_eq!(
        out,
        Value::Ref(Some(
            include_str!("fixtures/live_objects/javap_facts.txt")
                .trim()
                .to_owned()
        ))
    );
}
