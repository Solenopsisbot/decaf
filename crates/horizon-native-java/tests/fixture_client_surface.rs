//! Source-built Fabric and NeoForge client jars register key mappings, client
//! tick listeners and HUD elements the way real client mods do; the client
//! surface names each one, answers `consumeClick` from the key state it is
//! given, and records what a HUD element draws.
//!
//! The loader and game classes are compiled as stubs so `javac` accepts the
//! mods; only the mods' own classes go into the jars, so the native host
//! answers the API classes, exactly as it does for a real jar.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{
    ExecutionBudget, Host, HostKind, NativeError, NativeJar, StdHost, Value,
    client_surface::{
        ClientKeyMapping, ClientLevel, ClientMessage, ClientPlayer, ClientSnapshot, ClientStack,
        ClientText, DrawOp, KeyPress, TextMetrics,
    },
    events::{EventContext, EventFacade, EventFiring, EventLoader, GameEvent, GameListener},
    screen_models::{ScreenSlot, ScreenSlotKind},
};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
use zip::write::SimpleFileOptions;

fn scratch(name: &str) -> scratch::Scratch {
    let root = scratch::Scratch::new(&format!("client-surface-{name}"));
    fs::create_dir_all(&root).unwrap();
    root
}

/// Compile `sources` and pack the classes under `demo/` into a jar with
/// `metadata` as its loader file. `None` without a Java toolchain.
fn build_jar(root: &Path, sources: &[(&str, &str)], metadata: (&str, &str)) -> Option<PathBuf> {
    let src = root.join("src");
    let classes = root.join("classes");
    let mut paths = Vec::new();
    for (path, text) in sources {
        let file = src.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, text).unwrap();
        paths.push(file);
    }
    fs::create_dir_all(&classes).unwrap();
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "17", "-d"])
        .arg(&classes)
        .args(&paths)
        .status()
        .ok()?;
    assert!(status.success(), "javac failed");
    let jar_path = root.join("mod.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    zip.start_file(metadata.0, SimpleFileOptions::default())
        .unwrap();
    zip.write_all(metadata.1.as_bytes()).unwrap();
    for entry in fs::read_dir(classes.join("demo")).unwrap() {
        let path = entry.unwrap().path();
        let name = format!("demo/{}", path.file_name().unwrap().to_string_lossy());
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(&path).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    Some(jar_path)
}

fn static_value(host: &StdHost, owner: &str, key: &str) -> Option<Value> {
    host.persistent_class_state()
        .0
        .remove(&format!("{owner}#{key}"))
}

/// A frame in a world with a player at `x`, or at the title screen.
fn frame(x: Option<f64>) -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        player: x.map(|x| ClientPlayer {
            name: "Alex".into(),
            pos: [x, 64.0, -3.0],
            health: 20.0,
            max_health: 20.0,
            ..ClientPlayer::default()
        }),
        gui_size: [320, 240],
        metrics: Arc::new(TextMetrics {
            advances: vec![6.0; 128],
            translations: BTreeMap::new(),
            answers: Default::default(),
        }),
        ..ClientSnapshot::default()
    })
}

/// Fire `event` at every listener of it, inside one client frame.
fn fire(
    jar: &NativeJar,
    host: &mut StdHost,
    listeners: &[GameListener],
    event: GameEvent,
    snapshot: Arc<ClientSnapshot>,
    keys: &[KeyPress],
) -> horizon_native_java::client_surface::ClientFrameOutput {
    host.begin_client_frame(snapshot, keys);
    let mut firing = EventFiring::new(
        event,
        Arc::new(EventFacade::default()),
        EventContext::default(),
    );
    for listener in listeners.iter().filter(|l| l.event == event) {
        jar.run_game_listener(listener, &mut firing, host, &ExecutionBudget::UNBOUNDED)
            .unwrap();
    }
    host.end_client_frame()
}

const GAME_STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources;\n\
         public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { return null; } }",
    ),
    (
        "net/minecraft/network/chat/FormattedText.java",
        "package net.minecraft.network.chat;\npublic interface FormattedText {}",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat;\n\
         public interface Component extends FormattedText { static MutableComponent literal(String s) { return null; }\n\
           static MutableComponent translatable(String s) { return null; } }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat;\npublic interface MutableComponent extends Component {}",
    ),
    (
        "com/mojang/blaze3d/platform/InputConstants.java",
        "package com.mojang.blaze3d.platform;\n\
         public final class InputConstants { public static final int KEY_C = 6;\n\
           public enum Type { KEYBOARD, MOUSE } }",
    ),
    (
        "net/minecraft/client/KeyMapping.java",
        "package net.minecraft.client;\n\
         public class KeyMapping {\n\
           public KeyMapping(String n, int k, Category c) {}\n\
           public KeyMapping(String n, com.mojang.blaze3d.platform.InputConstants.Type t, int k, Category c) {}\n\
           public boolean consumeClick() { return false; } public boolean isDown() { return false; }\n\
           public String getName() { return null; }\n\
           public record Category(net.minecraft.resources.Identifier id) {\n\
             public static final Category MISC = null;\n\
             public static Category register(net.minecraft.resources.Identifier id) { return null; } } }",
    ),
    (
        "net/minecraft/client/gui/Font.java",
        "package net.minecraft.client.gui;\n\
         public class Font { public final int lineHeight = 9; public int width(String s) { return 0; }\n\
           public int width(net.minecraft.network.chat.FormattedText t) { return 0; } }",
    ),
    (
        "com/mojang/renderpearl/api/pipeline/RenderPipeline.java",
        "package com.mojang.renderpearl.api.pipeline;\npublic class RenderPipeline {}",
    ),
    (
        "net/minecraft/client/renderer/RenderPipelines.java",
        "package net.minecraft.client.renderer;\n\
         import com.mojang.renderpearl.api.pipeline.RenderPipeline;\n\
         public final class RenderPipelines {\n\
           public static final RenderPipeline GUI_TEXTURED = null;\n\
           public static final RenderPipeline GUI_OPAQUE_TEXTURED_BACKGROUND = null; }",
    ),
    (
        "net/minecraft/client/gui/GuiGraphicsExtractor.java",
        "package net.minecraft.client.gui;\n\
         import com.mojang.renderpearl.api.pipeline.RenderPipeline;\n\
         import net.minecraft.resources.Identifier;\n\
         public class GuiGraphicsExtractor {\n\
           public void text(Font f, String s, int x, int y, int c) {}\n\
           public void text(Font f, net.minecraft.network.chat.Component s, int x, int y, int c, boolean shadow) {}\n\
           public void centeredText(Font f, String s, int x, int y, int c) {}\n\
           public void centeredText(Font f, net.minecraft.network.chat.Component s, int x, int y, int c) {}\n\
           public void fill(int a, int b, int c, int d, int e) {}\n\
           public void blit(RenderPipeline p, Identifier id, int x, int y, float u, float v, int w, int h, int tw, int th) {}\n\
           public void blit(RenderPipeline p, Identifier id, int x, int y, float u, float v, int w, int h, int uw, int vh, int tw, int th) {}\n\
           public int guiWidth() { return 0; } }",
    ),
    (
        "net/minecraft/client/DeltaTracker.java",
        "package net.minecraft.client;\npublic interface DeltaTracker {}",
    ),
    (
        "net/minecraft/client/player/LocalPlayer.java",
        "package net.minecraft.client.player;\n\
         public class LocalPlayer { public double getX() { return 0; }\n\
           public void displayClientMessage(net.minecraft.network.chat.Component c, boolean overlay) {} }",
    ),
    (
        "net/minecraft/client/Minecraft.java",
        "package net.minecraft.client;\n\
         public class Minecraft { public net.minecraft.client.player.LocalPlayer player;\n\
           public net.minecraft.client.gui.Font font;\n\
           public static Minecraft getInstance() { return null; } }",
    ),
];

const FABRIC_STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/fabric/api/event/Event.java",
        "package net.fabricmc.fabric.api.event;\n\
         public abstract class Event<T> { public void register(T listener) {} }",
    ),
    (
        "net/fabricmc/fabric/api/client/event/lifecycle/v1/ClientTickEvents.java",
        "package net.fabricmc.fabric.api.client.event.lifecycle.v1;\n\
         public final class ClientTickEvents {\n\
           public static final net.fabricmc.fabric.api.event.Event<EndTick> END_CLIENT_TICK = null;\n\
           public interface EndTick { void onEndTick(net.minecraft.client.Minecraft client); } }",
    ),
    (
        "net/fabricmc/fabric/api/client/keymapping/v1/KeyMappingHelper.java",
        "package net.fabricmc.fabric.api.client.keymapping.v1;\n\
         public final class KeyMappingHelper {\n\
           public static net.minecraft.client.KeyMapping registerKeyMapping(net.minecraft.client.KeyMapping k) { return k; } }",
    ),
    (
        "net/fabricmc/fabric/api/client/rendering/v1/hud/HudElement.java",
        "package net.fabricmc.fabric.api.client.rendering.v1.hud;\n\
         public interface HudElement { void extractRenderState(net.minecraft.client.gui.GuiGraphicsExtractor g, net.minecraft.client.DeltaTracker d); }",
    ),
    (
        "net/fabricmc/fabric/api/client/rendering/v1/hud/HudElementRegistry.java",
        "package net.fabricmc.fabric.api.client.rendering.v1.hud;\n\
         public interface HudElementRegistry {\n\
           static void addLast(net.minecraft.resources.Identifier id, HudElement e) {} }",
    ),
];

const SCREEN_API_STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/world/flag/FeatureFlag.java",
        "package net.minecraft.world.flag; public class FeatureFlag {}",
    ),
    (
        "net/minecraft/world/flag/FeatureFlagSet.java",
        "package net.minecraft.world.flag;\n\
         public final class FeatureFlagSet {\n\
           public static FeatureFlagSet of() { return null; }\n\
           public static FeatureFlagSet of(FeatureFlag flag) { return null; }\n\
           public boolean isEmpty() { return false; } }",
    ),
    (
        "net/minecraft/world/flag/FeatureFlags.java",
        "package net.minecraft.world.flag;\n\
         public final class FeatureFlags {\n\
           public static final FeatureFlag VANILLA = null;\n\
           public static final FeatureFlag TRADE_REBALANCE = null;\n\
           public static final FeatureFlagSet VANILLA_SET = null;\n\
           public static boolean isExperimental(FeatureFlagSet features) { return false; } }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item; public final class ItemStack { public static final ItemStack EMPTY = null; }",
    ),
    (
        "net/minecraft/world/Container.java",
        "package net.minecraft.world; import net.minecraft.world.item.ItemStack; public interface Container { default ItemStack getItem(int slot) { return ItemStack.EMPTY; } }",
    ),
    (
        "net/minecraft/world/SimpleContainer.java",
        "package net.minecraft.world; public class SimpleContainer implements Container { public SimpleContainer(int size) {} }",
    ),
    (
        "net/minecraft/world/entity/player/Player.java",
        "package net.minecraft.world.entity.player; public class Player {}",
    ),
    (
        "net/minecraft/world/entity/player/Inventory.java",
        "package net.minecraft.world.entity.player;\n\
         import net.minecraft.network.chat.Component;\n\
         public class Inventory implements net.minecraft.world.Container {\n\
           public final Player player = null;\n\
           public Inventory() {}\n\
           public Component getName() { return null; }\n\
           public Component getDisplayName() { return getName(); } }",
    ),
    (
        "net/minecraft/world/inventory/ContainerLevelAccess.java",
        "package net.minecraft.world.inventory; public interface ContainerLevelAccess { ContainerLevelAccess NULL = null; }",
    ),
    (
        "net/minecraft/world/inventory/ContainerListener.java",
        "package net.minecraft.world.inventory; import net.minecraft.world.item.ItemStack; public interface ContainerListener { void slotChanged(AbstractContainerMenu m, int slot, ItemStack stack); void dataChanged(AbstractContainerMenu m, int id, int value); }",
    ),
    (
        "net/minecraft/world/inventory/Slot.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class Slot {\n\
           public final Container container; public int index; public final int x, y;\n\
           public Slot(Container c, int i, int x, int y) { container = c; this.x = x; this.y = y; }\n\
           public ItemStack getItem() { return container.getItem(0); } }",
    ),
    (
        "net/minecraft/core/NonNullList.java",
        "package net.minecraft.core;\n\
         import java.util.AbstractList;\n\
         public class NonNullList<E> extends AbstractList<E> {\n\
           @Override public E get(int i) { return null; }\n\
           @Override public int size() { return 0; } }",
    ),
    (
        "net/minecraft/world/inventory/ResultContainer.java",
        "package net.minecraft.world.inventory; import net.minecraft.world.Container; public class ResultContainer implements Container {}",
    ),
    (
        "net/minecraft/world/inventory/AbstractContainerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.core.NonNullList;\n\
         import net.minecraft.world.Container;\n\
         public abstract class AbstractContainerMenu {\n\
           public final NonNullList<Slot> slots = null;\n\
           protected AbstractContainerMenu(MenuType<?> t, int id) {}\n\
           protected Slot addSlot(Slot s) { return s; }\n\
           protected void addInventoryExtendedSlots(Container c, int x, int y) {}\n\
           protected void addInventoryHotbarSlots(Container c, int x, int y) {}\n\
           protected void addStandardInventorySlots(Container c, int x, int y) {}\n\
           public void addSlotListener(ContainerListener l) {}\n\
           public void removeSlotListener(ContainerListener l) {}\n\
           public void broadcastChanges() {}\n\
           public MenuType<?> getType() { return null; } }",
    ),
    (
        "net/minecraft/world/inventory/MenuType.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.flag.FeatureFlagSet;\n\
         public class MenuType<T extends AbstractContainerMenu> {\n\
           public interface MenuSupplier<T extends AbstractContainerMenu> { T create(int id, Inventory inv); }\n\
           public MenuType(MenuSupplier<T> s, FeatureFlagSet f) {}\n\
           public FeatureFlagSet requiredFeatures() { return null; }\n\
           public T create(int id, Inventory inv) { return null; } }",
    ),
    (
        "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition.java",
        "package net.minecraft.world.inventory; import java.util.function.Predicate; public class ItemCombinerMenuSlotDefinition { public static Builder create() { return null; } public static class Builder { public Builder withSlot(int i,int x,int y,Predicate<Object> p) { return this; } public Builder withResultSlot(int i,int x,int y) { return this; } public ItemCombinerMenuSlotDefinition build() { return null; } } }",
    ),
    (
        "net/minecraft/world/inventory/ItemCombinerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         public abstract class ItemCombinerMenu extends AbstractContainerMenu {\n\
           protected final Container inputSlots = null;\n\
           protected final ResultContainer resultSlots = null;\n\
           protected ItemCombinerMenu(MenuType<?> t,int id,Inventory i,ContainerLevelAccess a,ItemCombinerMenuSlotDefinition d) { super(t, id); } }",
    ),
    (
        "net/minecraft/client/gui/screens/MenuScreens.java",
        "package net.minecraft.client.gui.screens;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         public final class MenuScreens {\n\
           public interface ScreenConstructor<M extends AbstractContainerMenu, S extends Screen> { S create(M m, Inventory i, Component t); }\n\
           public static <M extends AbstractContainerMenu, S extends Screen> void register(MenuType<? extends M> t, ScreenConstructor<M, S> c) {} }",
    ),
    (
        "net/minecraft/client/gui/screens/Screen.java",
        "package net.minecraft.client.gui.screens;\n\
         import net.minecraft.client.Minecraft;\n\
         import net.minecraft.client.gui.Font;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.network.chat.Component;\n\
         public abstract class Screen {\n\
           protected final Component title;\n\
           protected final Minecraft minecraft;\n\
           protected final Font font;\n\
           public int width;\n\
           public int height;\n\
           protected Screen(Component t) { this.title = t; this.minecraft = null; this.font = null; }\n\
           public final void init(int w, int h) {}\n\
           protected void init() {}\n\
           public void added() {}\n\
           public void removed() {}\n\
           public void triggerImmediateNarration(boolean b) {}\n\
           public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {}\n\
           public void extractTransparentBackground(GuiGraphicsExtractor g) {}\n\
           public boolean isInGameUi() { return false; } }",
    ),
    (
        "net/minecraft/client/gui/screens/inventory/AbstractContainerScreen.java",
        "package net.minecraft.client.gui.screens.inventory;\n\
         import net.minecraft.client.gui.screens.Screen;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         public abstract class AbstractContainerScreen<T extends AbstractContainerMenu> extends Screen {\n\
           protected final int imageWidth;\n\
           protected final int imageHeight;\n\
           protected int titleLabelX;\n\
           protected int titleLabelY;\n\
           protected int inventoryLabelX;\n\
           protected int inventoryLabelY;\n\
           protected final T menu;\n\
           protected final Component playerInventoryTitle;\n\
           protected int leftPos;\n\
           protected int topPos;\n\
           protected AbstractContainerScreen(T m, Inventory i, Component t) { this(m, i, t, 176, 166); }\n\
           protected AbstractContainerScreen(T m, Inventory i, Component t, int w, int h) {\n\
             super(t); this.menu = m; this.playerInventoryTitle = i.getDisplayName(); this.imageWidth = w; this.imageHeight = h; } }",
    ),
    (
        "net/minecraft/client/gui/screens/inventory/ItemCombinerScreen.java",
        "package net.minecraft.client.gui.screens.inventory;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.ItemCombinerMenu;\n\
         public abstract class ItemCombinerScreen<T extends ItemCombinerMenu> extends AbstractContainerScreen<T> {\n\
           protected ItemCombinerScreen(T m, Inventory i, Component t, Identifier id) { super(m, i, t); }\n\
           protected void subInit() {}\n\
           protected void init() { super.init(); subInit(); }\n\
           public void removed() { super.removed(); }\n\
           public void slotChanged(net.minecraft.world.inventory.AbstractContainerMenu m, int slot, net.minecraft.world.item.ItemStack stack) {}\n\
           public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) { super.extractBackground(g, mx, my, pt); }\n\
           protected abstract void extractErrorIcon(GuiGraphicsExtractor g, int x, int y); }",
    ),
];

const FABRIC_MOD: &str = r#"
package demo;
import com.mojang.blaze3d.platform.InputConstants;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElementRegistry;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
public final class ClientMod {
  public static KeyMapping ZOOM;
  public static int presses;
  public static int ticks;
  public void onInitializeClient() {
    KeyMapping.Category category = KeyMapping.Category.register(Identifier.fromNamespaceAndPath("demo", "main"));
    ZOOM = KeyMappingHelper.registerKeyMapping(
        new KeyMapping("key.demo.zoom", InputConstants.Type.KEYBOARD, InputConstants.KEY_C, category));
    ClientTickEvents.END_CLIENT_TICK.register(client -> {
      ticks++;
      while (ZOOM.consumeClick()) {
        presses++;
        client.player.displayClientMessage(Component.literal("zoom " + presses), false);
      }
    });
    HudElementRegistry.addLast(Identifier.fromNamespaceAndPath("demo", "coords"), (graphics, delta) -> {
      Minecraft mc = Minecraft.getInstance();
      if (mc.player == null) return;
      graphics.text(mc.font, "X " + (int) mc.player.getX(), 2, 2, 0xFFFFFFFF);
      graphics.fill(0, 0, graphics.guiWidth(), 1, 0x80000000);
    });
  }
}
"#;

/// A mapping's name is the jar's own `String`, whatever it spells: a name written like a client
/// object's marker (`client/minecraft`) or a category's never comes back as that object (#232's
/// review). 26.3's `getName()` is `getfield name` (javap of the client jar); Java 25 printed the
/// expected line for the same listener over that shape: the argument equals the literal,
/// has its length, is `==` to it (interned), and is no `Minecraft`.
const NAMED_MOD: &str = r#"
package demo;
import com.mojang.blaze3d.platform.InputConstants;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
public final class ClientMod {
  public static KeyMapping SPELLED;
  public static KeyMapping CATEGORY;
  public static String seen = "";
  public void onInitializeClient() {
    SPELLED = KeyMappingHelper.registerKeyMapping(
        new KeyMapping("client/minecraft", InputConstants.Type.KEYBOARD, InputConstants.KEY_C, KeyMapping.Category.MISC));
    CATEGORY = KeyMappingHelper.registerKeyMapping(
        new KeyMapping("key-category/minecraft:misc", InputConstants.Type.KEYBOARD, InputConstants.KEY_C, KeyMapping.Category.MISC));
    ClientTickEvents.END_CLIENT_TICK.register(client -> {
      String a = SPELLED.getName();
      String b = CATEGORY.getName();
      seen = a.equals("client/minecraft") + "," + a.length() + "," + (a == "client/minecraft") + ","
          + ((Object) a == Minecraft.getInstance()) + "," + (a instanceof String) + "|"
          + b.equals("key-category/minecraft:misc") + "," + b.length() + ","
          + ((Object) b == KeyMapping.Category.MISC);
    });
  }
}
"#;

#[test]
fn a_mappings_name_is_its_own_text_whatever_it_spells() {
    let root = scratch("named");
    let mut sources = GAME_STUBS.to_vec();
    sources.extend_from_slice(FABRIC_STUBS);
    sources.push(("demo/ClientMod.java", NAMED_MOD));
    let Some(jar_path) = build_jar(
        &root,
        &sources,
        (
            "fabric.mod.json",
            r#"{"id":"demo","version":"1","environment":"client","entrypoints":{"client":["demo.ClientMod"]}}"#,
        ),
    ) else {
        eprintln!("skipped: no javac");
        return;
    };
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let entry = jar.discover_entrypoint("client").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "client", &mut host)
        .unwrap();
    let client = host.game_listeners(&jar, false);
    fire(
        &jar,
        &mut host,
        &client.listeners,
        GameEvent::ClientTickEnd,
        frame(Some(0.0)),
        &[KeyPress::default(), KeyPress::default()],
    );
    // What Java 25 prints for the same listener.
    assert_eq!(
        static_value(&host, "demo/ClientMod", "seenLjava/lang/String;"),
        Some(Value::Ref(Some(
            "true,16,true,false,true|true,27,false".into()
        )))
    );
}

/// A client frame in a world with both a player and a client level.
fn frame_in_world(x: f64) -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        player: Some(ClientPlayer {
            name: "Alex".into(),
            pos: [x, 64.0, -3.0],
            health: 20.0,
            max_health: 20.0,
            ..ClientPlayer::default()
        }),
        level: Some(ClientLevel {
            dimension: "minecraft:overworld".into(),
            ..ClientLevel::default()
        }),
        gui_size: [320, 240],
        metrics: Arc::new(TextMetrics {
            advances: vec![6.0; 128],
            translations: BTreeMap::new(),
            answers: Default::default(),
        }),
        ..ClientSnapshot::default()
    })
}

fn assert_standard_player_slots(slots: &[ScreenSlot], start_index: i32, top_y: i32) {
    assert_eq!(slots.len(), 36);
    for row in 0..3 {
        for col in 0..9 {
            let offset = (row * 9 + col) as usize;
            assert_eq!(
                slots[offset],
                ScreenSlot {
                    index: start_index + offset as i32,
                    x: 8 + 18 * col,
                    y: top_y + 18 * row,
                    kind: ScreenSlotKind::PlayerInventory,
                }
            );
        }
    }
    for col in 0..9 {
        let offset = 27 + col as usize;
        assert_eq!(
            slots[offset],
            ScreenSlot {
                index: start_index + offset as i32,
                x: 8 + 18 * col,
                y: top_y + 58,
                kind: ScreenSlotKind::PlayerInventory,
            }
        );
    }
}

/// Covers:
/// - Two distinct `MenuType`s with two screens (no null wildcard)
/// - `ItemCombinerScreen` with `subInit()` override changing `titleLabelX`,
///   `extractBackground` override calling `super.extractBackground` + blitting an
///   extra overlay with `RenderPipelines.GUI_TEXTURED` and float `u`/`v`, plus a
///   capturing lambda `MenuScreens$ScreenConstructor`
/// - `AbstractContainerScreen` 5-arg constructor (176x133 -> `inventoryLabelY == 39`),
///   `AbstractContainerMenu.addSlot` + `addStandardInventorySlots`, `init()` override
///   centering `titleLabelX` using `font.width(title)`, subclass field shadowing `leftPos`
///   without overwriting `AbstractContainerScreen.leftPos`, `<init>` write to `leftPos`
///   not shadowing `init()`, and `extractBackground` calling `super.extractBackground` +
///   `graphics.blit(RenderPipelines.GUI_TEXTURED, ...)`
#[test]
fn container_screens_run_full_init_super_background_lambdas_and_two_menus() {
    let root = scratch("screen-bridge-full");
    let mut sources = GAME_STUBS.to_vec();
    sources.extend_from_slice(SCREEN_API_STUBS);
    sources.extend_from_slice(&[
        (
            "demo/CombinerMenu.java",
            r#"package demo;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.*;
public class CombinerMenu extends ItemCombinerMenu {
  public static final MenuType<CombinerMenu> TYPE =
      new MenuType<>(CombinerMenu::new, FeatureFlags.VANILLA_SET);
  public CombinerMenu(int id, Inventory inv) {
    super(TYPE, id, inv, ContainerLevelAccess.NULL,
        ItemCombinerMenuSlotDefinition.create()
            .withSlot(0, 27, 47, s -> true)
            .withSlot(1, 76, 47, s -> true)
            .withResultSlot(2, 134, 47)
            .build());
  }
}
"#,
        ),
        (
            "demo/CombinerScreen.java",
            r#"package demo;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.ItemCombinerScreen;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Inventory;
public class CombinerScreen extends ItemCombinerScreen<CombinerMenu> {
  public static final Identifier BASE_TEX =
      Identifier.fromNamespaceAndPath("demo", "textures/gui/combiner.png");
  public static final Identifier OVERLAY_TEX =
      Identifier.fromNamespaceAndPath("demo", "textures/gui/overlay.png");
  private final int capturedTitleX;
  public CombinerScreen(CombinerMenu m, Inventory i, Component t, int titleX) {
    super(m, i, t, BASE_TEX);
    this.capturedTitleX = titleX;
    this.leftPos = 999; // must be overwritten by AbstractContainerScreen.init(), not shadowed
    this.titleLabelY = 19;
  }
  @Override
  protected void subInit() {
    this.titleLabelX = this.capturedTitleX;
  }
  @Override
  public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {
    super.extractBackground(g, mx, my, pt);
    g.blit(RenderPipelines.GUI_TEXTURED, OVERLAY_TEX,
        this.leftPos + 10, this.topPos + 12, 4.5f, 2.25f, 16, 14, 64, 64);
  }
  @Override
  protected void extractErrorIcon(GuiGraphicsExtractor g, int x, int y) {
    g.fill(x + 1, y + 2, x + 5, y + 6, 0xFF112233);
  }
}
"#,
        ),
        (
            "demo/HopperLikeMenu.java",
            r#"package demo;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.*;
public class HopperLikeMenu extends AbstractContainerMenu {
  public static final MenuType<HopperLikeMenu> TYPE =
      new MenuType<>(HopperLikeMenu::new, FeatureFlags.VANILLA_SET);
  public HopperLikeMenu(int id, Inventory inv) {
    super(TYPE, id);
    SimpleContainer box = new SimpleContainer(5);
    for (int i = 0; i < 5; i++) {
      this.addSlot(new Slot(box, i, 44 + i * 18, 20));
    }
    this.addStandardInventorySlots(inv, 8, 51);
  }
}
"#,
        ),
        (
            "demo/HopperLikeScreen.java",
            r#"package demo;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Inventory;
public class HopperLikeScreen extends AbstractContainerScreen<HopperLikeMenu> {
  public static final Identifier TEX =
      Identifier.fromNamespaceAndPath("demo", "textures/gui/hopper.png");
  // Shadowing subclass field named leftPos must never clobber AbstractContainerScreen.leftPos.
  private int leftPos;
  public HopperLikeScreen(HopperLikeMenu m, Inventory i, Component t) {
    super(m, i, t, 176, 133);
    this.leftPos = 12345;
  }
  @Override
  protected void init() {
    super.init();
    this.titleLabelX = (this.imageWidth - this.font.width(this.title)) / 2;
  }
  @Override
  public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {
    super.extractBackground(g, mx, my, pt);
    int x = (this.width - this.imageWidth) / 2;
    int y = (this.height - this.imageHeight) / 2;
    g.blit(RenderPipelines.GUI_TEXTURED, TEX, x, y, 0.0f, 0.0f, this.imageWidth, this.imageHeight, 256, 256);
  }
}
"#,
        ),
        (
            "demo/TwoScreensMod.java",
            r#"package demo;
import net.minecraft.client.gui.screens.MenuScreens;
public class TwoScreensMod {
  public void onInitializeClient() {
    int customTitleX = 42;
    // Capturing lambda factory for CombinerMenu:
    MenuScreens.register(CombinerMenu.TYPE, (m, i, t) -> new CombinerScreen(m, i, t, customTitleX));
    // Method-reference factory for HopperLikeMenu:
    MenuScreens.register(HopperLikeMenu.TYPE, HopperLikeScreen::new);
  }
}
"#,
        ),
    ]);
    let Some(jar_path) = build_jar(
        &root,
        &sources,
        (
            "fabric.mod.json",
            r#"{"id":"demo-two-screens","version":"1","environment":"client","entrypoints":{"client":["demo.TwoScreensMod"]}}"#,
        ),
    ) else {
        eprintln!("skipped: no javac");
        return;
    };
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let entry = jar.discover_entrypoint("client").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "client", &mut host)
        .unwrap();
    let combiner_type = static_value(
        &host,
        "demo/CombinerMenu",
        "TYPELnet/minecraft/world/inventory/MenuType;",
    )
    .expect("CombinerMenu.TYPE initialized");
    let hopper_type = static_value(
        &host,
        "demo/HopperLikeMenu",
        "TYPELnet/minecraft/world/inventory/MenuType;",
    )
    .expect("HopperLikeMenu.TYPE initialized");

    host.begin_client_frame(frame_in_world(0.0), &[]);
    // Construct BOTH menus via MenuType.create(containerId, clientInventory).
    let _combiner_menu = jar
        .create_client_menu(&mut host, combiner_type.clone(), 1)
        .unwrap();
    let _hopper_menu = jar
        .create_client_menu(&mut host, hopper_type.clone(), 2)
        .unwrap();

    // Null menu type is NEVER a wildcard.
    assert!(
        jar.open_client_screen(&mut host, Value::Ref(None), Value::Ref(Some("Test".into())))
            .is_err()
    );

    // 1. CombinerScreen: capturing lambda, subInit(), super.extractBackground + extractErrorIcon + overlay blit.
    let combiner_layout = jar
        .open_client_screen(
            &mut host,
            combiner_type.clone(),
            Value::Ref(Some("Combiner".into())),
        )
        .unwrap();
    // 26.3 AbstractContainerScreen.<init> 3-arg (0-13) -> 176x166, inventoryLabelY = 166 - 94 = 72;
    // AbstractContainerScreen.init (0-27) -> leftPos = (320-176)/2 = 72, topPos = (240-166)/2 = 37;
    // ItemCombinerScreen.init (4-5) -> CombinerScreen.subInit sets titleLabelX = 42 (captured).
    assert_eq!(combiner_layout.layout.image_width, 176);
    assert_eq!(combiner_layout.layout.image_height, 166);
    assert_eq!(combiner_layout.layout.left_pos, 72);
    assert_eq!(combiner_layout.layout.top_pos, 37);
    assert_eq!(combiner_layout.layout.title_label, [42, 19]);
    assert_eq!(combiner_layout.layout.inventory_label, [8, 72]);
    assert_eq!(
        combiner_layout.layout.menu_resource.as_deref(),
        Some("demo:textures/gui/combiner.png")
    );
    assert_eq!(combiner_layout.layout.slots.len(), 39);
    assert_eq!(
        &combiner_layout.layout.slots[..3],
        &[
            ScreenSlot {
                index: 0,
                x: 27,
                y: 47,
                kind: ScreenSlotKind::Container,
            },
            ScreenSlot {
                index: 1,
                x: 76,
                y: 47,
                kind: ScreenSlotKind::Container,
            },
            ScreenSlot {
                index: 2,
                x: 134,
                y: 47,
                kind: ScreenSlotKind::Result,
            },
        ]
    );
    assert_standard_player_slots(&combiner_layout.layout.slots[3..], 3, 84);

    let combiner_draws = jar
        .draw_client_screen_bg(
            &mut host,
            combiner_type,
            Value::Ref(Some("Combiner".into())),
        )
        .unwrap();
    assert_eq!(
        combiner_draws,
        vec![
            // From super.extractBackground (ItemCombinerScreen offsets 9-41):
            DrawOp::Blit {
                texture: "demo:textures/gui/combiner.png".into(),
                src: [0.0, 0.0, 176.0, 166.0],
                dst: [72.0, 37.0, 176.0, 166.0],
                tex_size: [256, 256],
                tint: u32::MAX,
            },
            // From super.extractBackground -> virtual extractErrorIcon (ItemCombinerScreen offsets 44-54):
            DrawOp::Fill {
                rect: [73.0, 39.0, 4.0, 4.0],
                argb: 0xFF11_2233,
            },
            // From CombinerScreen.extractBackground after super:
            DrawOp::Blit {
                texture: "demo:textures/gui/overlay.png".into(),
                src: [4.5, 2.25, 16.0, 14.0],
                dst: [82.0, 49.0, 16.0, 14.0],
                tex_size: [64, 64],
                tint: u32::MAX,
            },
        ]
    );

    // 2. HopperLikeScreen: 5-arg constructor (176x133 -> inventoryLabelY = 39),
    // init() override centering "Hopper" (6 chars * 6px = 36px -> (176 - 36) / 2 = 70),
    // subclass `private int leftPos = 12345` does NOT clobber AbstractContainerScreen.leftPos (72),
    // AbstractContainerMenu.addSlot (5 Container slots) + addStandardInventorySlots (36 PlayerInventory slots).
    // The title is a component, as the embedder hands one (`mod/src/menu_screen.rs`
    // `title_value`): the font measures a `FormattedText` only exactly.
    let hopper_title =
        horizon_native_java::screen_models::component_from_json(r#"{"text":"Hopper"}"#)
            .expect("a literal component");
    let hopper_layout = jar
        .open_client_screen(&mut host, hopper_type.clone(), hopper_title.clone())
        .unwrap();
    assert_eq!(hopper_layout.layout.image_width, 176);
    assert_eq!(hopper_layout.layout.image_height, 133);
    assert_eq!(hopper_layout.layout.left_pos, 72);
    assert_eq!(hopper_layout.layout.top_pos, 53); // (240 - 133) / 2 = 53
    assert_eq!(hopper_layout.layout.title_label, [70, 6]);
    assert_eq!(hopper_layout.layout.inventory_label, [8, 39]); // 133 - 94 = 39
    assert_eq!(hopper_layout.layout.menu_resource, None);
    assert_eq!(hopper_layout.layout.slots.len(), 41);
    for i in 0..5 {
        assert_eq!(
            hopper_layout.layout.slots[i],
            ScreenSlot {
                index: i as i32,
                x: 44 + (i as i32) * 18,
                y: 20,
                kind: ScreenSlotKind::Container,
            }
        );
    }
    assert_standard_player_slots(&hopper_layout.layout.slots[5..], 5, 51);

    let hopper_draws = jar
        .draw_client_screen_bg(&mut host, hopper_type, hopper_title)
        .unwrap();
    assert_eq!(
        hopper_draws,
        vec![DrawOp::Blit {
            texture: "demo:textures/gui/hopper.png".into(),
            src: [0.0, 0.0, 176.0, 133.0],
            dst: [72.0, 53.0, 176.0, 133.0],
            tex_size: [256, 256],
            tint: u32::MAX,
        }]
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "unexpected unknown calls: {:?}",
        host.unknown_host_calls()
    );
}

/// Covers counted refusals:
/// - unsupported 12-arg `GuiGraphicsExtractor.blit` overload
/// - unmodelled `RenderPipeline` passed to the 10-arg `blit`
/// - screen overriding `isInGameUi()` / `extractTransparentBackground(...)` (backdrop boundary)
#[test]
fn unsupported_blit_overload_pipeline_and_backdrop_overrides_are_counted() {
    let root = scratch("screen-bridge-counted");
    let mut sources = GAME_STUBS.to_vec();
    sources.extend_from_slice(SCREEN_API_STUBS);
    sources.extend_from_slice(&[
        (
            "demo/SimpleMenu.java",
            r#"package demo;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.*;
public class SimpleMenu extends AbstractContainerMenu {
  public static final MenuType<SimpleMenu> TYPE_OVERLOAD =
      new MenuType<>((id, inv) -> new SimpleMenu(SimpleMenu.TYPE_OVERLOAD, id), FeatureFlags.VANILLA_SET);
  public static final MenuType<SimpleMenu> TYPE_PIPELINE =
      new MenuType<>((id, inv) -> new SimpleMenu(SimpleMenu.TYPE_PIPELINE, id), FeatureFlags.VANILLA_SET);
  public static final MenuType<SimpleMenu> TYPE_BACKDROP =
      new MenuType<>((id, inv) -> new SimpleMenu(SimpleMenu.TYPE_BACKDROP, id), FeatureFlags.VANILLA_SET);
  public static final MenuType<SimpleMenu> TYPE_TRANSPARENT =
      new MenuType<>((id, inv) -> new SimpleMenu(SimpleMenu.TYPE_TRANSPARENT, id), FeatureFlags.VANILLA_SET);
  public SimpleMenu(MenuType<?> t, int id) { super(t, id); }
}
"#,
        ),
        (
            "demo/OverloadBlitScreen.java",
            r#"package demo;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Inventory;
public class OverloadBlitScreen extends AbstractContainerScreen<SimpleMenu> {
  private static final Identifier TEX = Identifier.fromNamespaceAndPath("demo", "textures/gui/a.png");
  public OverloadBlitScreen(SimpleMenu m, Inventory i, Component t) { super(m, i, t); }
  @Override
  public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {
    super.extractBackground(g, mx, my, pt);
    // 12-arg blit overload is not modelled -> counted.
    g.blit(RenderPipelines.GUI_TEXTURED, TEX, 0, 0, 0f, 0f, 16, 16, 32, 32, 256, 256);
  }
}
"#,
        ),
        (
            "demo/WrongPipelineScreen.java",
            r#"package demo;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Inventory;
public class WrongPipelineScreen extends AbstractContainerScreen<SimpleMenu> {
  private static final Identifier TEX = Identifier.fromNamespaceAndPath("demo", "textures/gui/a.png");
  public WrongPipelineScreen(SimpleMenu m, Inventory i, Component t) { super(m, i, t); }
  @Override
  public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {
    super.extractBackground(g, mx, my, pt);
    // Unmodelled pipeline -> counted.
    g.blit(RenderPipelines.GUI_OPAQUE_TEXTURED_BACKGROUND, TEX, 0, 0, 0f, 0f, 16, 16, 256, 256);
  }
}
"#,
        ),
        (
            "demo/CustomBackdropScreen.java",
            r#"package demo;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Inventory;
public class CustomBackdropScreen extends AbstractContainerScreen<SimpleMenu> {
  public CustomBackdropScreen(SimpleMenu m, Inventory i, Component t) { super(m, i, t); }
  @Override
  public boolean isInGameUi() { return false; }
  @Override
  public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {
    super.extractBackground(g, mx, my, pt);
  }
}
"#,
        ),
        (
            "demo/CustomTransparentBackdropScreen.java",
            r#"package demo;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Inventory;
public class CustomTransparentBackdropScreen extends AbstractContainerScreen<SimpleMenu> {
  public CustomTransparentBackdropScreen(SimpleMenu m, Inventory i, Component t) { super(m, i, t); }
  @Override
  public void extractTransparentBackground(GuiGraphicsExtractor g) {
    g.fill(0, 0, 10, 10, 0x80000000);
  }
  @Override
  public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {
    super.extractBackground(g, mx, my, pt);
  }
}
"#,
        ),
        (
            "demo/CountedMod.java",
            r#"package demo;
import net.minecraft.client.gui.screens.MenuScreens;
public class CountedMod {
  public void onInitializeClient() {
    MenuScreens.register(SimpleMenu.TYPE_OVERLOAD, OverloadBlitScreen::new);
    MenuScreens.register(SimpleMenu.TYPE_PIPELINE, WrongPipelineScreen::new);
    MenuScreens.register(SimpleMenu.TYPE_BACKDROP, CustomBackdropScreen::new);
    MenuScreens.register(SimpleMenu.TYPE_TRANSPARENT, CustomTransparentBackdropScreen::new);
  }
}
"#,
        ),
    ]);
    let Some(jar_path) = build_jar(
        &root,
        &sources,
        (
            "fabric.mod.json",
            r#"{"id":"demo-counted","version":"1","environment":"client","entrypoints":{"client":["demo.CountedMod"]}}"#,
        ),
    ) else {
        eprintln!("skipped: no javac");
        return;
    };
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let entry = jar.discover_entrypoint("client").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "client", &mut host)
        .unwrap();
    let type_overload = static_value(
        &host,
        "demo/SimpleMenu",
        "TYPE_OVERLOADLnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();
    let type_pipeline = static_value(
        &host,
        "demo/SimpleMenu",
        "TYPE_PIPELINELnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();
    let type_backdrop = static_value(
        &host,
        "demo/SimpleMenu",
        "TYPE_BACKDROPLnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();
    let type_transparent = static_value(
        &host,
        "demo/SimpleMenu",
        "TYPE_TRANSPARENTLnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();

    host.begin_client_frame(frame_in_world(0.0), &[]);
    jar.create_client_menu(&mut host, type_overload.clone(), 1)
        .unwrap();
    jar.create_client_menu(&mut host, type_pipeline.clone(), 2)
        .unwrap();
    jar.create_client_menu(&mut host, type_backdrop.clone(), 3)
        .unwrap();
    jar.create_client_menu(&mut host, type_transparent.clone(), 4)
        .unwrap();

    // 1. Unsupported 12-arg blit overload refuses and is counted.
    let err = jar
        .draw_client_screen_bg(&mut host, type_overload, Value::Ref(Some("A".into())))
        .unwrap_err();
    assert!(
        matches!(err, NativeError::UnknownHostCall(ref m) if m.name == "blit" && m.descriptor.contains("IIFFIIIIII")),
        "expected counted 12-arg blit, got {err:?}"
    );
    assert!(
        host.unknown_host_calls()
            .iter()
            .any(|(call, _)| call.contains("GuiGraphicsExtractor#blit")
                && call.contains("IIFFIIIIII"))
    );

    // 2. Unmodelled pipeline field / argument is counted.
    let before = host.unknown_host_call_count();
    let pipeline_result =
        jar.draw_client_screen_bg(&mut host, type_pipeline, Value::Ref(Some("B".into())));
    assert!(
        matches!(pipeline_result, Err(NativeError::UnknownHostCall(_)))
            || host.unknown_host_call_count() > before,
        "expected unmodelled pipeline to be counted, got {pipeline_result:?}"
    );

    // 3. Screen overriding isInGameUi() breaks the host-backdrop boundary and is counted.
    let before = host.unknown_host_call_count();
    let _ = jar
        .draw_client_screen_bg(&mut host, type_backdrop, Value::Ref(Some("C".into())))
        .unwrap();
    assert!(
        host.unknown_host_call_count() > before,
        "expected isInGameUi override to be counted"
    );
    assert!(
        host.unknown_host_calls()
            .iter()
            .any(|(call, _)| call == "demo/CustomBackdropScreen#isInGameUi()Z"),
        "expected demo/CustomBackdropScreen#isInGameUi()Z in {:?}",
        host.unknown_host_calls()
    );

    // 4. Screen overriding extractTransparentBackground(...) breaks the host-backdrop boundary and is counted.
    let before = host.unknown_host_call_count();
    let _ = jar
        .draw_client_screen_bg(&mut host, type_transparent, Value::Ref(Some("D".into())))
        .unwrap();
    assert!(
        host.unknown_host_call_count() > before,
        "expected extractTransparentBackground override to be counted"
    );
    assert!(
        host.unknown_host_calls().iter().any(|(call, _)| {
            call == "demo/CustomTransparentBackdropScreen#extractTransparentBackground(Lnet/minecraft/client/gui/GuiGraphicsExtractor;)V"
        }),
        "expected extractTransparentBackground override in {:?}",
        host.unknown_host_calls()
    );
}

/// Round-2 regression suite covering:
/// - `super.added()` handled cleanly + `triggerImmediateNarration(false)` override called at Screen.init(int, int) offset 39
/// - `open_client_screen` -> `draw_client_screen_bg` reuses Screen #1 (no duplicate init/listener leak);
///   switching screens runs `removed()` and removes Screen #1's listener
/// - `broadcastChanges`: virtual `Slot.getItem()` override, `Objects.requireNonNull` NPE on null stack,
///   counted `getItem` counts `broadcastChanges` instead of storing/firing null, client inventory 0..41
///   (including armor/offhand) fires `slotChanged` once on initial non-empty slot and counts ambiguous repeat
/// - `ItemCombinerMenu.<init>`: virtual `addSlot`, `addStandardInventorySlots` -> `addInventoryExtendedSlots` /
///   `addInventoryHotbarSlots`, NPE on null inventory, and counted `Inventory.player` when frame has no player
/// - `Inventory.getDisplayName()` / `getName()`: canonical `DEFAULT_NAME` (`==` holds), jar `Inventory`
///   subclass override respected, missing client player counted
/// - `blit`: `tex_w <= 0 || tex_h <= 0` counted, `wrapping_add` for `x + w` / `y + h` without panic
#[test]
fn round2_screen_lifecycle_broadcast_overrides_inventory_and_blit_guards() {
    let root = scratch("screen-bridge-round2");
    let mut sources = GAME_STUBS.to_vec();
    sources.extend_from_slice(SCREEN_API_STUBS);
    sources.extend_from_slice(&[
        (
            "demo/CustomSlot.java",
            r#"package demo;
import net.minecraft.world.Container;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
public class CustomSlot extends Slot {
  public static int getItemCalls = 0;
  private final int mode;
  public CustomSlot(Container c, int i, int x, int y, int mode) {
    super(c, i, x, y);
    this.mode = mode;
  }
  @Override
  public ItemStack getItem() {
    getItemCalls++;
    if (mode == 1) return null; // triggers Objects.requireNonNull NPE (offset 30)
    if (mode == 2) return super.getItem(); // calls unmodelled container -> counted
    return ItemStack.EMPTY;
  }
}
"#,
        ),
        (
            "demo/HookCombinerMenu.java",
            r#"package demo;
import net.minecraft.world.Container;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.*;
public class HookCombinerMenu extends ItemCombinerMenu {
  public static final MenuType<HookCombinerMenu> TYPE =
      new MenuType<>(HookCombinerMenu::new, FeatureFlags.VANILLA_SET);
  public static int addSlotCount = 0;
  public static int extCount = 0;
  public static int hotbarCount = 0;
  public HookCombinerMenu(int id, Inventory inv) {
    super(TYPE, id, inv, ContainerLevelAccess.NULL,
        ItemCombinerMenuSlotDefinition.create()
            .withSlot(0, 27, 47, s -> true)
            .withResultSlot(1, 134, 47)
            .build());
    // Also add an offhand slot (index 40) to exercise 0..41 client inventory broadcast.
    this.addSlot(new Slot(inv, 40, 152, 47));
  }
  @Override
  protected Slot addSlot(Slot s) {
    addSlotCount++;
    return super.addSlot(s);
  }
  @Override
  protected void addInventoryExtendedSlots(Container c, int x, int y) {
    extCount++;
    super.addInventoryExtendedSlots(c, x, y);
  }
  @Override
  protected void addInventoryHotbarSlots(Container c, int x, int y) {
    hotbarCount++;
    super.addInventoryHotbarSlots(c, x, y);
  }
}
"#,
        ),
        (
            "demo/LifecycleScreen.java",
            r#"package demo;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.ItemCombinerScreen;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.item.ItemStack;
public class LifecycleScreen extends ItemCombinerScreen<HookCombinerMenu> {
  public static final Identifier TEX =
      Identifier.fromNamespaceAndPath("demo", "textures/gui/life.png");
  public static int ctorCount = 0;
  public static int addedCount = 0;
  public static int removedCount = 0;
  public static int narrationCalls = 0;
  public static boolean narrationArg = true;
  public static int slotChangedCalls = 0;
  public static int lastChangedSlot = -1;
  public static boolean canonicalInvTitle = false;
  public LifecycleScreen(HookCombinerMenu m, Inventory i, Component t) {
    super(m, i, t, TEX);
    ctorCount++;
    canonicalInvTitle = (this.playerInventoryTitle == i.getDisplayName())
        && (i.getDisplayName() == i.getName());
  }
  @Override
  public void added() {
    super.added();
    addedCount++;
  }
  @Override
  public void removed() {
    super.removed();
    removedCount++;
  }
  @Override
  public void triggerImmediateNarration(boolean b) {
    narrationCalls++;
    narrationArg = b;
  }
  @Override
  public void slotChanged(AbstractContainerMenu m, int slot, ItemStack stack) {
    slotChangedCalls++;
    lastChangedSlot = slot;
  }
  @Override
  protected void extractErrorIcon(GuiGraphicsExtractor g, int x, int y) {
    // Exercise wrapping_add on large coordinates without overflow panic.
    g.blit(RenderPipelines.GUI_TEXTURED, TEX, Integer.MAX_VALUE, 0, 0f, 0f, 1, 1, 256, 256);
  }
}
"#,
        ),
        (
            "demo/BadTexBlitScreen.java",
            r#"package demo;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.inventory.*;
import net.minecraft.world.flag.FeatureFlags;
public class BadTexBlitScreen extends AbstractContainerScreen<BadTexBlitScreen.Menu> {
  public static final Identifier TEX = Identifier.fromNamespaceAndPath("demo", "textures/gui/bad.png");
  public static class Menu extends AbstractContainerMenu {
    public static final MenuType<Menu> TYPE =
        new MenuType<>((id, inv) -> new Menu(Menu.TYPE, id), FeatureFlags.VANILLA_SET);
    public Menu(MenuType<?> t, int id) { super(t, id); }
  }
  public BadTexBlitScreen(Menu m, Inventory i, Component t) { super(m, i, t); }
  public Component getPlayerInventoryTitle() { return this.playerInventoryTitle; }
  @Override
  public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {
    super.extractBackground(g, mx, my, pt);
    g.blit(RenderPipelines.GUI_TEXTURED, TEX, 0, 0, 0f, 0f, 16, 16, 0, 256);
  }
}
"#,
        ),
        (
            "demo/CustomInventory.java",
            r#"package demo;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Inventory;
public class CustomInventory extends Inventory {
  public static final Component CUSTOM_NAME = Component.literal("Custom Bag");
  @Override
  public Component getName() {
    return CUSTOM_NAME;
  }
}
"#,
        ),
        (
            "demo/BroadcastMenus.java",
            r#"package demo;
import net.minecraft.network.chat.Component;
import net.minecraft.world.Container;
import net.minecraft.world.inventory.*;
public class BroadcastMenus {
  public static class SlotTestMenu extends AbstractContainerMenu {
    public SlotTestMenu(int mode) {
      super(null, 1);
      Container c = new Container() {};
      this.addSlot(new CustomSlot(c, 0, 0, 0, mode));
    }
  }
  public static boolean checkNullInventoryThrowsNpe() {
    try {
      new HookCombinerMenu(1, null);
      return false;
    } catch (NullPointerException expected) {
      return true;
    }
  }
  public static boolean checkNullSlotGetItemThrowsNpe() {
    SlotTestMenu m = new SlotTestMenu(1);
    try {
      m.broadcastChanges();
      return false;
    } catch (NullPointerException expected) {
      return true;
    }
  }
  public static void runCustomEmptySlotBroadcast() {
    SlotTestMenu m = new SlotTestMenu(0);
    m.broadcastChanges();
  }
  public static void runUnmodelledContainerBroadcast() {
    SlotTestMenu m = new SlotTestMenu(2);
    m.broadcastChanges();
  }
  public static void runBroadcastOn(AbstractContainerMenu m) {
    m.broadcastChanges();
  }
  public static boolean checkCustomInventorySubclassOverride() {
    CustomInventory inv = new CustomInventory();
    BadTexBlitScreen.Menu m = new BadTexBlitScreen.Menu(BadTexBlitScreen.Menu.TYPE, 1);
    BadTexBlitScreen screen = new BadTexBlitScreen(m, inv, Component.literal("T"));
    return screen.getPlayerInventoryTitle() == CustomInventory.CUSTOM_NAME;
  }
  public static void triggerResidentGcWithThrowawayArrays() {
    for (int i = 0; i < 8250; i++) {
      int[] trash = new int[1];
      trash[0] = i;
    }
  }
  public static class Box {
    public int value;
    public Box(int v) { this.value = v; }
  }
  public static int allocateAndUseFreshJarObject(int v) {
    Box b = new Box(v);
    b.value = b.value + 1;
    return b.value;
  }
}
"#,
        ),
        (
            "demo/Round2Mod.java",
            r#"package demo;
import net.minecraft.client.gui.screens.MenuScreens;
public class Round2Mod {
  public void onInitializeClient() {
    MenuScreens.register(HookCombinerMenu.TYPE, LifecycleScreen::new);
    MenuScreens.register(BadTexBlitScreen.Menu.TYPE, BadTexBlitScreen::new);
  }
}
"#,
        ),
    ]);
    let Some(jar_path) = build_jar(
        &root,
        &sources,
        (
            "fabric.mod.json",
            r#"{"id":"demo-round2","version":"1","environment":"client","entrypoints":{"client":["demo.Round2Mod"]}}"#,
        ),
    ) else {
        eprintln!("skipped: no javac");
        return;
    };
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let entry = jar.discover_entrypoint("client").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "client", &mut host)
        .unwrap();
    let hook_type = static_value(
        &host,
        "demo/HookCombinerMenu",
        "TYPELnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();
    let bad_type = static_value(
        &host,
        "demo/BadTexBlitScreen$Menu",
        "TYPELnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();

    // Frame with an item in offhand (slot 40) to test 0..41 client inventory broadcast.
    let mut snap = (*frame_in_world(0.0)).clone();
    snap.player.as_mut().unwrap().offhand = ClientStack {
        item: "minecraft:shield".into(),
        count: 1,
        max_stack: 1,
        name: "Shield".into(),
        ..ClientStack::default()
    };
    host.begin_client_frame(Arc::new(snap), &[]);

    let hook_menu = jar
        .create_client_menu(&mut host, hook_type.clone(), 1)
        .unwrap();
    // 1 input + 1 result + 36 standard player slots + 1 extra offhand slot = 39 addSlot calls;
    // addStandardInventorySlots virtually dispatched both extended (1) and hotbar (1).
    assert_eq!(
        static_value(&host, "demo/HookCombinerMenu", "addSlotCountI"),
        Some(Value::Int(39))
    );
    assert_eq!(
        static_value(&host, "demo/HookCombinerMenu", "extCountI"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/HookCombinerMenu", "hotbarCountI"),
        Some(Value::Int(1))
    );

    // open_client_screen + draw_client_screen_bg: reuses Screen #1, runs super.added(),
    // runs triggerImmediateNarration(false), fires slotChanged once for offhand slot 38,
    // verifies canonical DEFAULT_NAME (`==`), and handles wrapping_add in blit without panic.
    let _layout = jar
        .open_client_screen(
            &mut host,
            hook_type.clone(),
            Value::Ref(Some("Life".into())),
        )
        .unwrap();
    let draws = jar
        .draw_client_screen_bg(&mut host, hook_type, Value::Ref(Some("Life".into())))
        .unwrap();
    assert_eq!(draws.len(), 2);
    assert_eq!(
        static_value(&host, "demo/LifecycleScreen", "ctorCountI"),
        Some(Value::Int(1)),
        "draw_client_screen_bg must reuse Screen #1 instead of constructing a duplicate"
    );
    assert_eq!(
        static_value(&host, "demo/LifecycleScreen", "addedCountI"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/LifecycleScreen", "narrationCallsI"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/LifecycleScreen", "narrationArgZ"),
        Some(Value::Int(0))
    );
    assert_eq!(
        static_value(&host, "demo/LifecycleScreen", "slotChangedCallsI"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/LifecycleScreen", "lastChangedSlotI"),
        Some(Value::Int(38))
    );
    assert_eq!(
        static_value(&host, "demo/LifecycleScreen", "canonicalInvTitleZ"),
        Some(Value::Int(1))
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "unexpected unknown calls before counted checks: {:?}",
        host.unknown_host_calls()
    );

    // Calling broadcastChanges a second time while offhand still has 1x shield (same item & count):
    // ClientStack lacks DataComponentMap, so ItemStack.matches is counted rather than guessed.
    let before = host.unknown_host_call_count();
    let _ = jar.invoke_method_with_args(
        "demo/BroadcastMenus",
        "runBroadcastOn",
        "(Lnet/minecraft/world/inventory/AbstractContainerMenu;)V",
        None,
        &[hook_menu],
        &mut host,
    );
    assert!(
        host.unknown_host_call_count() > before,
        "expected ambiguous non-empty ItemStack.matches to be counted"
    );

    // Switching to BadTexBlitScreen calls LifecycleScreen.removed() (removing listener),
    // and tex_w == 0 in blit is refused and counted.
    jar.create_client_menu(&mut host, bad_type.clone(), 2)
        .unwrap();
    let bad_err = jar
        .draw_client_screen_bg(&mut host, bad_type, Value::Ref(Some("Bad".into())))
        .unwrap_err();
    assert!(matches!(bad_err, NativeError::UnknownHostCall(ref m) if m.name == "blit"));
    assert_eq!(
        static_value(&host, "demo/LifecycleScreen", "removedCountI"),
        Some(Value::Int(1)),
        "opening another screen must call removed() on the previous screen"
    );

    // Static checks: null inventory NPE, null Slot.getItem() NPE, virtual empty Slot.getItem(),
    // unmodelled Container.getItem counted, and jar Inventory subclass overriding getName().
    assert_eq!(
        jar.invoke_method_with_args(
            "demo/BroadcastMenus",
            "checkNullInventoryThrowsNpe",
            "()Z",
            None,
            &[],
            &mut host,
        )
        .unwrap(),
        Value::Int(1)
    );
    assert_eq!(
        jar.invoke_method_with_args(
            "demo/BroadcastMenus",
            "checkNullSlotGetItemThrowsNpe",
            "()Z",
            None,
            &[],
            &mut host,
        )
        .unwrap(),
        Value::Int(1)
    );
    let before = host.unknown_host_call_count();
    jar.invoke_method_with_args(
        "demo/BroadcastMenus",
        "runCustomEmptySlotBroadcast",
        "()V",
        None,
        &[],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.unknown_host_call_count(), before);
    assert!(
        matches!(
            static_value(&host, "demo/CustomSlot", "getItemCallsI"),
            Some(Value::Int(n)) if n >= 2
        ),
        "virtual Slot.getItem() must run for jar Slot subclasses"
    );
    let before = host.unknown_host_call_count();
    jar.invoke_method_with_args(
        "demo/BroadcastMenus",
        "runUnmodelledContainerBroadcast",
        "()V",
        None,
        &[],
        &mut host,
    )
    .unwrap();
    assert!(
        host.unknown_host_call_count() > before,
        "unmodelled Container.getItem in broadcastChanges must be counted"
    );
    assert_eq!(
        jar.invoke_method_with_args(
            "demo/BroadcastMenus",
            "checkCustomInventorySubclassOverride",
            "()Z",
            None,
            &[],
            &mut host,
        )
        .unwrap(),
        Value::Int(1)
    );

    // End-to-end Finding 1 check: after building menus (with #menu<n> host keys),
    // force a real resident-heap GC (8250 > MAX_RESIDENT_ARRAYS) and then allocate
    // and read/write fields on a brand-new jar object without hitting refuse_collected.
    jar.invoke_method_with_args(
        "demo/BroadcastMenus",
        "triggerResidentGcWithThrowawayArrays",
        "()V",
        None,
        &[],
        &mut host,
    )
    .unwrap();
    assert_eq!(
        jar.invoke_method_with_args(
            "demo/BroadcastMenus",
            "allocateAndUseFreshJarObject",
            "(I)I",
            None,
            &[Value::Int(41)],
            &mut host,
        )
        .unwrap(),
        Value::Int(42)
    );

    // Frame with NO player: ItemCombinerMenu.<init> counts Inventory.player (no silent null).
    host.begin_client_frame(Arc::new(ClientSnapshot::default()), &[]);
    let before = host.unknown_host_call_count();
    let hook_type_no_player = hook_type_for_no_player(&host);
    let _ = jar.create_client_menu(&mut host, hook_type_no_player, 3);
    assert!(
        host.unknown_host_call_count() > before,
        "creating ItemCombinerMenu with client/inventory and no player must count Inventory.player"
    );
}

fn hook_type_for_no_player(host: &StdHost) -> Value {
    static_value(
        host,
        "demo/HookCombinerMenu",
        "TYPELnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap()
}

#[test]
fn a_real_runes_client_registers_its_container_screen() {
    // The real Runes 1.3.2 jar isn't in the repo or on CI: point RUNES_JAR at it to run this.
    let Some(path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes screen test skipped");
        return;
    };
    let jar = NativeJar::from_path(std::path::Path::new(&path)).unwrap();
    let entry = jar
        .discover_entrypoint("client")
        .expect("Runes declares a Fabric client entrypoint");
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "client", &mut host)
        .expect("Runes discovered client entrypoint failed");
    let summary = host.client_registration_summary();
    assert!(
        summary["order"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "menu_screen")
    );
    let handler_type = static_value(
        &host,
        "net/runes/crafting/RuneCraftingScreenHandler",
        "HANDLER_TYPELnet/minecraft/world/inventory/MenuType;",
    )
    .expect("RuneCraftingScreenHandler.HANDLER_TYPE initialized");

    // Begin a client frame with a real client level and player, and construct
    // Runes' REAL RuneCraftingScreenHandler through its registered MenuType.
    host.begin_client_frame(frame_in_world(0.0), &[]);
    let _menu = jar
        .create_client_menu(&mut host, handler_type.clone(), 1)
        .expect("Runes RuneCraftingScreenHandler construction failed");

    let layout = jar
        .open_client_screen(
            &mut host,
            handler_type.clone(),
            Value::Ref(Some("Rune Crafting".into())),
        )
        .expect("open_client_screen failed on Runes");
    // 26.3 AbstractContainerScreen.<init> 3-arg (offsets 0-13) -> 176x166, inventoryLabelY = 166 - 94 = 72;
    // RuneCraftingScreen.<init> offsets 10-19 -> titleLabelX = 60, titleLabelY = 18;
    // AbstractContainerScreen.init offsets 0-27 -> leftPos = (320 - 176) / 2 = 72, topPos = (240 - 166) / 2 = 37.
    assert_eq!(layout.layout.image_width, 176);
    assert_eq!(layout.layout.image_height, 166);
    assert_eq!(layout.layout.title_label, [60, 18]);
    assert_eq!(layout.layout.inventory_label, [8, 72]);
    assert_eq!(layout.layout.left_pos, 72);
    assert_eq!(layout.layout.top_pos, 37);
    assert_eq!(
        layout.layout.menu_resource.as_deref(),
        Some("runes:textures/gui/crafting_altar.png")
    );
    // RuneCraftingScreenHandler.createForgingSlotsManager offsets 0-41:
    // input slots (27,47) and (76,47), result slot (134,47), plus 36 standard player slots at (8,84)/(8,142).
    assert_eq!(layout.layout.slots.len(), 39);
    assert_eq!(
        &layout.layout.slots[..3],
        &[
            ScreenSlot {
                index: 0,
                x: 27,
                y: 47,
                kind: ScreenSlotKind::Container,
            },
            ScreenSlot {
                index: 1,
                x: 76,
                y: 47,
                kind: ScreenSlotKind::Container,
            },
            ScreenSlot {
                index: 2,
                x: 134,
                y: 47,
                kind: ScreenSlotKind::Result,
            },
        ]
    );
    assert_standard_player_slots(&layout.layout.slots[3..], 3, 84);

    // ItemCombinerScreen.extractBackground offsets 9-41 -> exact Blit, then
    // RuneCraftingScreen.extractErrorIcon offset 0 -> return.
    let draws = jar
        .draw_client_screen_bg(
            &mut host,
            handler_type,
            Value::Ref(Some("Rune Crafting".into())),
        )
        .expect("draw_client_screen_bg failed on Runes");
    assert_eq!(
        draws,
        vec![DrawOp::Blit {
            texture: "runes:textures/gui/crafting_altar.png".into(),
            src: [0.0, 0.0, 176.0, 166.0],
            dst: [72.0, 37.0, 176.0, 166.0],
            tex_size: [256, 256],
            tint: u32::MAX,
        }]
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "Runes client screen run had counted calls: {:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn a_fabric_client_mod_gets_its_key_tick_listener_and_hud_element() {
    let root = scratch("fabric");
    let mut sources = GAME_STUBS.to_vec();
    sources.extend_from_slice(FABRIC_STUBS);
    sources.push(("demo/ClientMod.java", FABRIC_MOD));
    let Some(jar_path) = build_jar(
        &root,
        &sources,
        (
            "fabric.mod.json",
            r#"{"id":"demo","version":"1","environment":"client","entrypoints":{"client":["demo.ClientMod"]}}"#,
        ),
    ) else {
        eprintln!("skipped: no javac");
        return;
    };
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let entry = jar.discover_entrypoint("client").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "client", &mut host)
        .unwrap();

    // The tick listener is a client event: a client keeps it, a server
    // counts it as an event it never fires.
    let client = host.game_listeners(&jar, false);
    let named: Vec<(GameEvent, EventLoader, &str)> = client
        .listeners
        .iter()
        .map(|l| (l.event, l.loader, l.source.as_str()))
        .collect();
    assert_eq!(
        named,
        [(
            GameEvent::ClientTickEnd,
            EventLoader::Fabric,
            "ClientTickEvents.END_CLIENT_TICK"
        )]
    );
    let server = host.game_listeners(&jar, true);
    assert!(server.listeners.is_empty());
    assert_eq!(
        server.unsupported.get("ClientTickEvents.END_CLIENT_TICK"),
        Some(&1)
    );

    let zoom = static_value(
        &host,
        "demo/ClientMod",
        "ZOOMLnet/minecraft/client/KeyMapping;",
    )
    .and_then(|value| match value {
        // The mapping the jar `new`ed: an object handle, keyed `class#n`.
        Value::Host(handle) if handle.kind() == HostKind::JarObject => {
            Some(handle.key().to_owned())
        }
        _ => None,
    })
    .expect("registerKeyMapping hands the mapping back");
    assert_eq!(
        host.client_key_mappings(),
        &[ClientKeyMapping {
            marker: zoom,
            name: "key.demo.zoom".into(),
            default_key: "key.keyboard.c".into(),
            category: "demo:main".into(),
            source: "KeyMappingHelper.registerKeyMapping",
        }]
    );
    let (layers, refused) = host.client_hud_layers(&jar);
    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].id, "demo:coords");

    // Two clicks waiting: the listener consumes both and says so in chat.
    let out = fire(
        &jar,
        &mut host,
        &client.listeners,
        GameEvent::ClientTickEnd,
        frame(Some(12.7)),
        &[KeyPress {
            down: true,
            clicks: 2,
        }],
    );
    assert_eq!(
        out.messages,
        [
            ClientMessage::Chat(ClientText::Literal("zoom 1".into())),
            ClientMessage::Chat(ClientText::Literal("zoom 2".into())),
        ]
    );
    assert_eq!(
        out.keys,
        [KeyPress {
            down: true,
            clicks: 0
        }]
    );
    // No click: the tick still runs, nothing is consumed.
    let out = fire(
        &jar,
        &mut host,
        &client.listeners,
        GameEvent::ClientTickEnd,
        frame(Some(12.7)),
        &[KeyPress::default()],
    );
    assert!(out.messages.is_empty());
    assert_eq!(
        static_value(&host, "demo/ClientMod", "ticksI"),
        Some(Value::Int(2))
    );
    assert_eq!(
        static_value(&host, "demo/ClientMod", "pressesI"),
        Some(Value::Int(2))
    );

    // The HUD element draws the player's x and a bar the GUI's width.
    host.begin_client_frame(frame(Some(12.7)), &[]);
    jar.run_hud_layer(&layers[0], &mut host, &ExecutionBudget::UNBOUNDED)
        .unwrap();
    let out = host.end_client_frame();
    assert_eq!(
        out.draws,
        [
            DrawOp::Text {
                text: ClientText::Literal("X 12".into()),
                at: [2.0, 2.0],
                argb: 0xFFFF_FFFF,
                shadow: true,
                scale: [1.0, 1.0],
            },
            DrawOp::Fill {
                rect: [0.0, 0.0, 320.0, 1.0],
                argb: 0x8000_0000,
            },
        ]
    );
    // Without a world there is no player, and the element draws nothing.
    host.begin_client_frame(frame(None), &[]);
    jar.run_hud_layer(&layers[0], &mut host, &ExecutionBudget::UNBOUNDED)
        .unwrap();
    assert!(host.end_client_frame().draws.is_empty());
    let _ = fs::remove_dir_all(root);
}

const NEOFORGE_STUBS: &[(&str, &str)] = &[
    (
        "net/neoforged/fml/common/Mod.java",
        "package net.neoforged.fml.common;\n\
         import java.lang.annotation.*;\n\
         @Retention(RetentionPolicy.RUNTIME) public @interface Mod { String value(); }",
    ),
    (
        "net/neoforged/api/distmarker/Dist.java",
        "package net.neoforged.api.distmarker;\npublic enum Dist { CLIENT, DEDICATED_SERVER }",
    ),
    (
        "net/neoforged/fml/common/EventBusSubscriber.java",
        "package net.neoforged.fml.common;\n\
         import java.lang.annotation.*;\n\
         @Retention(RetentionPolicy.RUNTIME) public @interface EventBusSubscriber {\n\
           String modid() default \"\"; net.neoforged.api.distmarker.Dist[] value() default {}; }",
    ),
    (
        "net/neoforged/bus/api/SubscribeEvent.java",
        "package net.neoforged.bus.api;\n\
         import java.lang.annotation.*;\n\
         @Retention(RetentionPolicy.RUNTIME) public @interface SubscribeEvent {}",
    ),
    (
        "net/neoforged/bus/api/Event.java",
        "package net.neoforged.bus.api;\npublic abstract class Event {}",
    ),
    (
        "net/neoforged/neoforge/client/event/RegisterKeyMappingsEvent.java",
        "package net.neoforged.neoforge.client.event;\n\
         public class RegisterKeyMappingsEvent extends net.neoforged.bus.api.Event {\n\
           public void register(net.minecraft.client.KeyMapping k) {} }",
    ),
    (
        "net/neoforged/neoforge/client/gui/GuiLayer.java",
        "package net.neoforged.neoforge.client.gui;\n\
         public interface GuiLayer { void render(net.minecraft.client.gui.GuiGraphicsExtractor g, net.minecraft.client.DeltaTracker d); }",
    ),
    (
        "net/neoforged/neoforge/client/event/RegisterGuiLayersEvent.java",
        "package net.neoforged.neoforge.client.event;\n\
         public class RegisterGuiLayersEvent extends net.neoforged.bus.api.Event {\n\
           public void registerAboveAll(net.minecraft.resources.Identifier id, net.neoforged.neoforge.client.gui.GuiLayer l) {} }",
    ),
    (
        "net/neoforged/neoforge/client/event/ClientTickEvent.java",
        "package net.neoforged.neoforge.client.event;\n\
         public abstract class ClientTickEvent extends net.neoforged.bus.api.Event {\n\
           public static class Post extends ClientTickEvent {} }",
    ),
];

const NEOFORGE_MOD: &str = r#"
package demo;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.RegisterGuiLayersEvent;
import net.neoforged.neoforge.client.event.RegisterKeyMappingsEvent;
@Mod("demonf")
@EventBusSubscriber(modid = "demonf", value = Dist.CLIENT)
public final class NeoClient {
  public static final KeyMapping OPEN = new KeyMapping("key.demonf.open", 7, KeyMapping.Category.MISC);
  public static int held;
  @SubscribeEvent public static void keys(RegisterKeyMappingsEvent event) { event.register(OPEN); }
  @SubscribeEvent public static void layers(RegisterGuiLayersEvent event) {
    event.registerAboveAll(Identifier.fromNamespaceAndPath("demonf", "title"),
        (graphics, delta) -> graphics.text(Minecraft.getInstance().font,
            Component.translatable("hud.demonf.title"), 4, 6, -1, false));
  }
  @SubscribeEvent public static void tick(ClientTickEvent.Post event) { if (OPEN.isDown()) held++; }
}
"#;

#[test]
fn a_neoforge_client_mod_registers_through_its_mod_bus_events() {
    let root = scratch("neoforge");
    let mut sources = GAME_STUBS.to_vec();
    sources.extend_from_slice(NEOFORGE_STUBS);
    sources.push(("demo/NeoClient.java", NEOFORGE_MOD));
    let Some(jar_path) = build_jar(
        &root,
        &sources,
        (
            "META-INF/neoforge.mods.toml",
            "modLoader=\"javafml\"\nloaderVersion=\"[1,)\"\nlicense=\"MIT\"\n[[mods]]\nmodId=\"demonf\"\nversion=\"1.0\"\n",
        ),
    ) else {
        eprintln!("skipped: no javac");
        return;
    };
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let budget = horizon_native_java::ExecutionBudget::UNBOUNDED;
    let mut host = StdHost::new();
    jar.execute("demo.NeoClient", None, "neoforge", &mut host)
        .unwrap();
    // FML injects the `@EventBusSubscriber` after the `@Mod` constructor.
    assert!(
        jar.inject_event_subscribers(&mut host, &budget, &[])
            .is_empty()
    );
    let found = host.game_listeners(&jar, false);
    let mut named: Vec<GameEvent> = found.listeners.iter().map(|l| l.event).collect();
    named.sort();
    assert_eq!(
        named,
        [
            GameEvent::ClientTickEnd,
            GameEvent::ClientRegisterKeyMappings,
            GameEvent::ClientRegisterGuiLayers,
        ]
    );
    // A dedicated server never injects a `Dist.CLIENT` subscriber.
    let mut server = StdHost::new().with_environment_type("SERVER");
    jar.execute("demo.NeoClient", None, "neoforge", &mut server)
        .unwrap();
    assert!(
        jar.inject_event_subscribers(&mut server, &budget, &[])
            .is_empty()
    );
    let on_server = server.game_listeners(&jar, true);
    assert!(on_server.listeners.is_empty());
    assert!(
        on_server.unsupported.is_empty(),
        "{:?}",
        on_server.unsupported
    );

    // Nothing is registered until the client fires its registration events.
    assert!(host.client_key_mappings().is_empty());
    for event in [
        GameEvent::ClientRegisterKeyMappings,
        GameEvent::ClientRegisterGuiLayers,
    ] {
        fire(&jar, &mut host, &found.listeners, event, frame(None), &[]);
    }
    let keys = host.client_key_mappings();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        (
            keys[0].name.as_str(),
            keys[0].default_key.as_str(),
            keys[0].category.as_str(),
            keys[0].source
        ),
        (
            "key.demonf.open",
            "key.keyboard.d",
            "minecraft:misc",
            "RegisterKeyMappingsEvent.register"
        )
    );
    let (layers, _) = host.client_hud_layers(&jar);
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].id, "demonf:title");

    // `isDown` follows the key state: held for one tick, released the next.
    for down in [true, false] {
        fire(
            &jar,
            &mut host,
            &found.listeners,
            GameEvent::ClientTickEnd,
            frame(Some(0.0)),
            &[KeyPress { down, clicks: 0 }],
        );
    }
    assert_eq!(
        static_value(&host, "demo/NeoClient", "heldI"),
        Some(Value::Int(1))
    );
    host.begin_client_frame(frame(Some(0.0)), &[]);
    jar.run_hud_layer(&layers[0], &mut host, &ExecutionBudget::UNBOUNDED)
        .unwrap();
    assert_eq!(
        host.end_client_frame().draws,
        [DrawOp::Text {
            text: ClientText::Translatable("hud.demonf.title".into()),
            at: [4.0, 6.0],
            argb: 0xFFFF_FFFF,
            shadow: false,
            scale: [1.0, 1.0],
        }]
    );
    let _ = fs::remove_dir_all(root);
}

/// Round-3 regression suite covering:
/// 1. `#menu<n>` objects are real `HostKind::MenuObject` handles (not plain `String`s):
///    `(List) menu.slots`, `menu.slots instanceof List`, `!(menu.slots instanceof String)`,
///    `(String) menu.slots` throwing CCE, `menu.slots.getClass().getName()`, `menu.slots.size()`,
///    forged String `"net/minecraft/core/NonNullList#menu0"` staying a plain `String`,
///    and `StdHost::retain(&menu_slots)` pinning the handle.
/// 2. `broadcastChanges` runs a jar `SimpleContainer` subclass's `getItem(int)` override
///    instead of bypassing it via `entry.known_current`, and counts `broadcastChanges` if
///    that override hits an unmodelled call.
/// 3. Screen replacement order (`MenuScreens$ScreenConstructor.fromPacket` offset 22 ->
///    `Gui.setScreen` offsets 77/220/234/319): if the new screen constructor throws or
///    returns null, the previous active screen stays active and its `removed()` is NOT
///    called; on valid replacement, constructor runs BEFORE `prev.removed()`, which runs
///    BEFORE `new.added()` and `new.init()`.
/// 4. Private `ItemCombinerMenu.createContainer(int)` is not treated as overridable.
#[test]
fn round3_menu_object_handles_container_override_and_screen_replacement_order() {
    let root = scratch("screen-bridge-round3");
    let mut sources = GAME_STUBS.to_vec();
    sources.extend_from_slice(SCREEN_API_STUBS);
    sources.extend_from_slice(&[
        (
            "demo/OverrideContainer.java",
            r#"package demo;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.item.ItemStack;
public class OverrideContainer extends SimpleContainer {
  public static int getItemCalls = 0;
  private final boolean hitUnknown;
  public OverrideContainer(int size, boolean hitUnknown) {
    super(size);
    this.hitUnknown = hitUnknown;
  }
  @Override
  public ItemStack getItem(int slot) {
    getItemCalls++;
    if (hitUnknown) {
      // Unmodelled static call -> increments unknown_call_count, so broadcastChanges must count.
      System.gc();
      return ItemStack.EMPTY;
    }
    return super.getItem(slot);
  }
}
"#,
        ),
        (
            "demo/Round3Menu.java",
            r#"package demo;
import java.util.Collection;
import java.util.List;
import net.minecraft.core.NonNullList;
import net.minecraft.world.Container;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.*;
public class Round3Menu extends ItemCombinerMenu {
  public static final MenuType<Round3Menu> TYPE_A =
      new MenuType<>((id, inv) -> new Round3Menu(Round3Menu.TYPE_A, id, inv), FeatureFlags.VANILLA_SET);
  public static final MenuType<Round3Menu> TYPE_THROW =
      new MenuType<>((id, inv) -> new Round3Menu(Round3Menu.TYPE_THROW, id, inv), FeatureFlags.VANILLA_SET);
  public static final MenuType<Round3Menu> TYPE_NULL =
      new MenuType<>((id, inv) -> new Round3Menu(Round3Menu.TYPE_NULL, id, inv), FeatureFlags.VANILLA_SET);
  public static final MenuType<Round3Menu> TYPE_B =
      new MenuType<>((id, inv) -> new Round3Menu(Round3Menu.TYPE_B, id, inv), FeatureFlags.VANILLA_SET);
  public static final MenuType<Round3Menu> TYPE_UNKNOWN_CONTAINER =
      new MenuType<>((id, inv) -> new Round3Menu(Round3Menu.TYPE_UNKNOWN_CONTAINER, id, inv, true), FeatureFlags.VANILLA_SET);

  public static Object capturedSlots;
  public static boolean slotsIsList;
  public static boolean slotsIsCollection;
  public static boolean slotsIsNonNullList;
  public static boolean slotsIsString;
  public static boolean slotsCastToStringThrew;
  public static boolean forgedIsString;
  public static boolean forgedIsList;
  public static boolean forgedCastToListThrew;
  public static int castListSize;
  public static int directSlotsSize;
  public static String slotsClassName;
  public static boolean inputIsSimpleContainer;
  public static boolean inputIsString;
  public static boolean resultIsResultContainer;
  public static boolean resultIsString;
  public static boolean reqFeaturesMatchVanilla;
  public static boolean vanillaNotExperimental;
  public static boolean emptyNotExperimental;
  public static boolean tradeIsExperimental;
  public static String isExperimentalNullMsg;

  public Round3Menu(MenuType<?> t, int id, Inventory inv) {
    this(t, id, inv, false);
  }

  public Round3Menu(MenuType<?> t, int id, Inventory inv, boolean hitUnknown) {
    super(t, id, inv, ContainerLevelAccess.NULL,
        ItemCombinerMenuSlotDefinition.create()
            .withSlot(0, 27, 47, s -> true)
            .withResultSlot(1, 134, 47)
            .build());
    // Add a vanilla Slot backed by a jar SimpleContainer subclass overriding getItem(int).
    this.addSlot(new Slot(new OverrideContainer(1, hitUnknown), 0, 80, 47));
  }

  // Same name/descriptor as vanilla private ItemCombinerMenu.createContainer(int):
  // NOT an override in 26.3, so constructing Round3Menu must NOT count it.
  public SimpleContainer createContainer(int size) {
    throw new AssertionError("private vanilla method must not be overridden");
  }

  public void probeHandles() {
    Object o = this.slots;
    capturedSlots = o;
    slotsIsList = o instanceof List;
    slotsIsCollection = o instanceof Collection;
    slotsIsNonNullList = o instanceof NonNullList;
    slotsIsString = o instanceof String;
    List<?> asList = (List<?>) o;
    castListSize = asList.size();
    directSlotsSize = this.slots.size();
    slotsClassName = o.getClass().getName();
    try {
      String bad = (String) o;
      slotsCastToStringThrew = bad == null;
    } catch (ClassCastException e) {
      slotsCastToStringThrew = true;
    }

    Object forged = "net/minecraft/core/NonNullList#menu0";
    forgedIsString = forged instanceof String;
    forgedIsList = forged instanceof List;
    try {
      List<?> bad = (List<?>) forged;
      forgedCastToListThrew = bad == null;
    } catch (ClassCastException e) {
      forgedCastToListThrew = true;
    }

    Object inObj = this.inputSlots;
    inputIsSimpleContainer = inObj instanceof SimpleContainer && inObj instanceof Container;
    inputIsString = inObj instanceof String;
    Object resObj = this.resultSlots;
    resultIsResultContainer = resObj instanceof ResultContainer && resObj instanceof Container;
    resultIsString = resObj instanceof String;

    net.minecraft.world.flag.FeatureFlagSet req = Round3Menu.TYPE_A.requiredFeatures();
    reqFeaturesMatchVanilla = req == FeatureFlags.VANILLA_SET && req.equals(FeatureFlags.VANILLA_SET);
    vanillaNotExperimental = !FeatureFlags.isExperimental(req);
    emptyNotExperimental = !FeatureFlags.isExperimental(net.minecraft.world.flag.FeatureFlagSet.of());
    tradeIsExperimental = FeatureFlags.isExperimental(
        net.minecraft.world.flag.FeatureFlagSet.of(FeatureFlags.TRADE_REBALANCE));
    try {
      FeatureFlags.isExperimental(null);
      isExperimentalNullMsg = "no-throw";
    } catch (NullPointerException e) {
      isExperimentalNullMsg = e.getMessage();
    }
  }
}
"#,
        ),
        (
            "demo/ScreenOrderLog.java",
            r#"package demo;
public class ScreenOrderLog {
  public static String trace = "";
  public static int removedA = 0;
  public static int ctorA = 0;
  public static void log(String entry) {
    trace = trace.isEmpty() ? entry : trace + "," + entry;
  }
}
"#,
        ),
        (
            "demo/ScreenA.java",
            r#"package demo;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Inventory;
public class ScreenA extends AbstractContainerScreen<Round3Menu> {
  public ScreenA(Round3Menu m, Inventory i, Component t) {
    super(m, i, t);
    ScreenOrderLog.ctorA++;
    ScreenOrderLog.log("ctorA");
    m.probeHandles();
    m.broadcastChanges();
  }
  @Override
  public void added() {
    super.added();
    ScreenOrderLog.log("addedA");
  }
  @Override
  protected void init() {
    super.init();
    ScreenOrderLog.log("initA");
  }
  @Override
  public void removed() {
    ScreenOrderLog.removedA++;
    ScreenOrderLog.log("removedA");
    super.removed();
  }
}
"#,
        ),
        (
            "demo/ThrowingScreen.java",
            r#"package demo;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Inventory;
public class ThrowingScreen extends AbstractContainerScreen<Round3Menu> {
  public ThrowingScreen(Round3Menu m, Inventory i, Component t) {
    super(m, i, t);
    ScreenOrderLog.log("ctorThrow");
    throw new IllegalStateException("boom");
  }
}
"#,
        ),
        (
            "demo/ScreenB.java",
            r#"package demo;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Inventory;
public class ScreenB extends AbstractContainerScreen<Round3Menu> {
  public ScreenB(Round3Menu m, Inventory i, Component t) {
    super(m, i, t);
    ScreenOrderLog.log("ctorB");
  }
  @Override
  public void added() {
    super.added();
    ScreenOrderLog.log("addedB");
  }
  @Override
  protected void init() {
    super.init();
    ScreenOrderLog.log("initB");
  }
}
"#,
        ),
        (
            "demo/Round3Mod.java",
            r#"package demo;
import net.minecraft.client.gui.screens.MenuScreens;
public class Round3Mod {
  public void onInitializeClient() {
    MenuScreens.register(Round3Menu.TYPE_A, ScreenA::new);
    MenuScreens.register(Round3Menu.TYPE_THROW, ThrowingScreen::new);
    MenuScreens.register(Round3Menu.TYPE_NULL, (m, i, t) -> {
      ScreenOrderLog.log("ctorNull");
      return null;
    });
    MenuScreens.register(Round3Menu.TYPE_B, ScreenB::new);
    MenuScreens.register(Round3Menu.TYPE_UNKNOWN_CONTAINER, (m, i, t) -> {
      m.broadcastChanges();
      return new ScreenB(m, i, t);
    });
  }
}
"#,
        ),
    ]);
    let Some(jar_path) = build_jar(
        &root,
        &sources,
        (
            "fabric.mod.json",
            r#"{"id":"demo-round3","version":"1","environment":"client","entrypoints":{"client":["demo.Round3Mod"]}}"#,
        ),
    ) else {
        eprintln!("skipped: no javac");
        return;
    };
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let entry = jar.discover_entrypoint("client").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "client", &mut host)
        .unwrap();
    let type_a = static_value(
        &host,
        "demo/Round3Menu",
        "TYPE_ALnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();
    let type_throw = static_value(
        &host,
        "demo/Round3Menu",
        "TYPE_THROWLnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();
    let type_null = static_value(
        &host,
        "demo/Round3Menu",
        "TYPE_NULLLnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();
    let type_b = static_value(
        &host,
        "demo/Round3Menu",
        "TYPE_BLnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();

    host.begin_client_frame(frame_in_world(0.0), &[]);
    jar.create_client_menu(&mut host, type_a.clone(), 1)
        .unwrap();
    jar.create_client_menu(&mut host, type_throw.clone(), 2)
        .unwrap();
    jar.create_client_menu(&mut host, type_null.clone(), 3)
        .unwrap();
    jar.create_client_menu(&mut host, type_b.clone(), 4)
        .unwrap();

    // Opening ScreenA runs handle probes, broadcastChanges with OverrideContainer.getItem,
    // and private createContainer check (0 unknown calls!).
    jar.open_client_screen(&mut host, type_a.clone(), Value::Ref(Some("A".into())))
        .unwrap();

    // Finding 1 assertions:
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "slotsIsListZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "slotsIsCollectionZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "slotsIsNonNullListZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "slotsIsStringZ"),
        Some(Value::Int(0))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "slotsCastToStringThrewZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "forgedIsStringZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "forgedIsListZ"),
        Some(Value::Int(0))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "forgedCastToListThrewZ"),
        Some(Value::Int(1))
    );
    // 1 input + 1 result + 36 player inventory + 1 custom OverrideContainer slot = 39 slots.
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "castListSizeI"),
        Some(Value::Int(39))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "directSlotsSizeI"),
        Some(Value::Int(39))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "slotsClassNameLjava/lang/String;"),
        Some(Value::Ref(Some("net.minecraft.core.NonNullList".into())))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "inputIsSimpleContainerZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "inputIsStringZ"),
        Some(Value::Int(0))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "resultIsResultContainerZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "resultIsStringZ"),
        Some(Value::Int(0))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "reqFeaturesMatchVanillaZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "vanillaNotExperimentalZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "emptyNotExperimentalZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/Round3Menu", "tradeIsExperimentalZ"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(
            &host,
            "demo/Round3Menu",
            "isExperimentalNullMsgLjava/lang/String;"
        ),
        Some(Value::Ref(Some(
            "Cannot invoke \"net.minecraft.world.flag.FeatureFlagSet.isSubsetOf(net.minecraft.world.flag.FeatureFlagSet)\" because \"features\" is null"
                .into()
        )))
    );
    // StdHost::retain pins the MenuObject handle (not dropped as a plain String).
    let captured_slots =
        static_value(&host, "demo/Round3Menu", "capturedSlotsLjava/lang/Object;").unwrap();
    host.retain(&captured_slots);
    assert_eq!(host.retained_count(), 1);
    host.release(&captured_slots);
    assert_eq!(host.retained_count(), 0);

    // Finding 2 assertion: OverrideContainer.getItem(int) ran virtually during broadcastChanges!
    assert_eq!(
        static_value(&host, "demo/OverrideContainer", "getItemCallsI"),
        Some(Value::Int(1))
    );

    // Finding 5 + cleanliness: zero unknown calls across menu construction, probes, and broadcastChanges!
    assert!(
        host.unknown_host_calls().is_empty(),
        "unexpected unknown calls: {:?}",
        host.unknown_host_calls()
    );

    // Finding 3 assertions:
    // 3a. Throwing new screen constructor does NOT remove or clear ScreenA.
    let err = jar
        .open_client_screen(&mut host, type_throw, Value::Ref(Some("Throw".into())))
        .unwrap_err();
    assert!(
        matches!(err, NativeError::UncaughtException { ref class, .. } if class == "java/lang/IllegalStateException"),
        "expected IllegalStateException, got {err:?}"
    );
    assert_eq!(
        static_value(&host, "demo/ScreenOrderLog", "removedAI"),
        Some(Value::Int(0))
    );
    // ScreenA is still active: draw_client_screen_bg reuses it without calling ctorA again!
    jar.draw_client_screen_bg(&mut host, type_a.clone(), Value::Ref(Some("A".into())))
        .unwrap();
    assert_eq!(
        static_value(&host, "demo/ScreenOrderLog", "ctorAI"),
        Some(Value::Int(1))
    );

    // 3b. Null-returning screen constructor also does NOT remove or clear ScreenA.
    let err = jar
        .open_client_screen(&mut host, type_null, Value::Ref(Some("Null".into())))
        .unwrap_err();
    assert!(
        matches!(err, NativeError::Metadata(ref msg) if msg.contains("returned null")),
        "expected null constructor Metadata error, got {err:?}"
    );
    assert_eq!(
        static_value(&host, "demo/ScreenOrderLog", "removedAI"),
        Some(Value::Int(0))
    );
    jar.draw_client_screen_bg(&mut host, type_a, Value::Ref(Some("A".into())))
        .unwrap();
    assert_eq!(
        static_value(&host, "demo/ScreenOrderLog", "ctorAI"),
        Some(Value::Int(1))
    );

    // 3c. Valid replacement constructs ScreenB FIRST, THEN removes ScreenA, THEN runs addedB/initB.
    jar.open_client_screen(&mut host, type_b, Value::Ref(Some("B".into())))
        .unwrap();
    assert_eq!(
        static_value(&host, "demo/ScreenOrderLog", "removedAI"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/ScreenOrderLog", "traceLjava/lang/String;"),
        Some(Value::Ref(Some(
            "ctorA,addedA,initA,ctorThrow,ctorNull,ctorB,removedA,addedB,initB".into()
        )))
    );

    // 2b. Container.getItem(int) override hitting an unmodelled call counts broadcastChanges.
    let type_unknown_container = static_value(
        &host,
        "demo/Round3Menu",
        "TYPE_UNKNOWN_CONTAINERLnet/minecraft/world/inventory/MenuType;",
    )
    .unwrap();
    jar.create_client_menu(&mut host, type_unknown_container.clone(), 5)
        .unwrap();
    jar.open_client_screen(
        &mut host,
        type_unknown_container,
        Value::Ref(Some("Unk".into())),
    )
    .unwrap();
    assert!(
        host.unknown_host_calls().iter().any(|(call, _)| call
            == "net/minecraft/world/inventory/AbstractContainerMenu#broadcastChanges()V"),
        "expected counted broadcastChanges after container override hit unknown call, got {:?}",
        host.unknown_host_calls()
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn gui_graphics_null_string_draws_nothing_and_null_component_throws_npe() {
    const NULL_TEXT_MOD: &str = r#"
package demo;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElementRegistry;
import net.minecraft.client.Minecraft;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;

public final class NullTextHud {
  public static int nullStringOk;
  public static int nullCenteredStringOk;
  public static int nullComponentNpe;
  public static int nullCenteredComponentNpe;

  public void onInitializeClient() {
    HudElementRegistry.addLast(
        Identifier.fromNamespaceAndPath("demo", "null_text"),
        (graphics, delta) -> {
          var font = Minecraft.getInstance().font;
          graphics.text(font, (String) null, 0, 0, 0xFFFFFF);
          nullStringOk = 1;
          graphics.centeredText(font, (String) null, 10, 0, 0xFFFFFF);
          nullCenteredStringOk = 1;
          try {
            graphics.text(font, (Component) null, 0, 0, 0xFFFFFF, true);
          } catch (NullPointerException expected) {
            // Java 25's helpful message names the real parameter (str).
            nullComponentNpe =
                ("Cannot invoke \"net.minecraft.network.chat.Component.getVisualOrderText()\" because \"str\" is null")
                        .equals(expected.getMessage())
                    ? 1
                    : 2;
          }
          try {
            graphics.centeredText(font, (Component) null, 10, 0, 0xFFFFFF);
          } catch (NullPointerException expected) {
            // Java 25's helpful message names the real parameter (text).
            nullCenteredComponentNpe =
                ("Cannot invoke \"net.minecraft.network.chat.Component.getVisualOrderText()\" because \"text\" is null")
                        .equals(expected.getMessage())
                    ? 1
                    : 2;
          }
        });
  }
}
"#;

    let root = scratch("null-text-npe");
    let mut sources = Vec::new();
    sources.extend_from_slice(GAME_STUBS);
    sources.extend_from_slice(FABRIC_STUBS);
    sources.push(("demo/NullTextHud.java", NULL_TEXT_MOD));
    let Some(jar_path) = build_jar(
        &root,
        &sources,
        (
            "fabric.mod.json",
            r#"{"id":"demo","version":"1","environment":"client","entrypoints":{"client":["demo.NullTextHud"]}}"#,
        ),
    ) else {
        eprintln!("skipped: no javac");
        let _ = fs::remove_dir_all(root);
        return;
    };
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let entry = jar.discover_entrypoint("client").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "client", &mut host)
        .unwrap();
    let (layers, unsupported) = host.client_hud_layers(&jar);
    assert!(unsupported.is_empty(), "{unsupported:?}");
    assert_eq!(layers.len(), 1);

    host.begin_client_frame(frame(Some(0.0)), &[]);
    jar.run_hud_layer(&layers[0], &mut host, &ExecutionBudget::UNBOUNDED)
        .unwrap();
    let output = host.end_client_frame();
    assert!(
        output.draws.is_empty(),
        "null String/Component calls must draw nothing: {:?}",
        output.draws
    );
    assert_eq!(
        static_value(&host, "demo/NullTextHud", "nullStringOkI"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/NullTextHud", "nullCenteredStringOkI"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/NullTextHud", "nullComponentNpeI"),
        Some(Value::Int(1))
    );
    assert_eq!(
        static_value(&host, "demo/NullTextHud", "nullCenteredComponentNpeI"),
        Some(Value::Int(1))
    );
    let _ = fs::remove_dir_all(root);
}
