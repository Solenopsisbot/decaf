//! The client screen of a Java mod's menu (`client/menu_screen.rs`) driven the way Sunrise drives
//! a mod menu screen: the factory this mod registers is installed through the real part
//! registration (`install_parts_fixture`), the window asks for the screen through
//! `mod_api::__private::create_menu_screen` (the installed factory lookup the client's open uses),
//! and each frame, click and close goes through the `OwnedMenuScreen` the window owns.
//!
//! Screens are the jars' own: a javac-built `ItemCombinerScreen` shaped like Runes' (own
//! texture, moved title, an `extractErrorIcon`) over `live_menu_tests`' `demo.BenchMenu`, an
//! `AbstractContainerScreen` with its own background and two overrides Sunrise performs itself,
//! and the real Runes 1.3.2 altar screen (`RUNES_JAR`).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::__private::game::BackendRefOf;
use mod_api::client::hud_layout::Blend;
use mod_api::{
    Backend, BackendRef, ClientHost, DrawContext, GuiDraw, GuiPaintError, Hooks, Host,
    MenuScreenFrame, MenuScreenOpen, ModEntry, Registrar, RegistrationContext, Text,
};
use zip::write::SimpleFileOptions;

use crate::HorizonJava;
use crate::live_menu_tests::{DEMO_SOURCES, STUBS};

// ---------------------------------------------------------------------------
// The test host and draw context
// ---------------------------------------------------------------------------

/// A client with a window: `window_size` over `gui_scale` is the GUI size `Screen.init` gets.
struct ScreenHost {
    window: [u32; 2],
    scale: u32,
}

impl ScreenHost {
    fn new(window: [u32; 2], scale: u32) -> Self {
        Self { window, scale }
    }
}

impl Host for ScreenHost {
    fn backend(&self) -> Option<BackendRef<'_>> {
        Some(BackendRef::of(self))
    }
}
impl Backend for ScreenHost {}
mod_api::impl_backend_areas!(ScreenHost);
mod_api::impl_host_areas!(ScreenHost);
impl ClientHost for ScreenHost {}
mod_api::impl_client_host_areas!(ScreenHost, except(client_options, menus));

/// No logged-in player handle: the screen frame then names the player by no UUID.
impl mod_api::prelude::MenusClientHost for ScreenHost {
    fn local_player(&self) -> Option<mod_api::PlayerRef> {
        None
    }
}

impl mod_api::prelude::ClientOptionsClientHost for ScreenHost {
    fn gui_scale(&self) -> Option<u32> {
        Some(self.scale)
    }
    fn window_size(&self) -> Option<[u32; 2]> {
        Some(self.window)
    }
}

/// The window's draw context for one frame: what the screen painted.
#[derive(Default)]
struct Painted(Vec<GuiDraw>);

impl DrawContext for Painted {
    fn gui_paint(&mut self, draws: &[GuiDraw]) -> Result<(), GuiPaintError> {
        self.0.extend_from_slice(draws);
        Ok(())
    }
}

/// A frame of the window: Sunrise centres the panel (`batch.centered_origin`), as
/// `AbstractContainerScreen.init` does.
fn frame(gui_size: [i32; 2], panel: [i32; 2]) -> MenuScreenFrame<'static> {
    let origin = [
        ((gui_size[0] - panel[0]) / 2) as f32,
        ((gui_size[1] - panel[1]) / 2) as f32,
    ];
    MenuScreenFrame {
        origin,
        pointer: [100, 100],
        gui_size,
        at: [100.0 - origin[0], 100.0 - origin[1]],
        data: &[],
    }
}

fn open<'a>(window: i32, menu: &'a str, title: &'a Text, backing: usize) -> MenuScreenOpen<'a> {
    MenuScreenOpen {
        window,
        menu,
        block: None,
        pos: None,
        title: Some(title),
        backing_slots: backing,
        player_slots: 36,
        data_slots: 0,
        extra: &[],
    }
}

/// `blit(GUI_TEXTURED, texture, x, y, 0, 0, w, h, 256, 256)` as Sunrise's op.
fn blit(texture: &'static str, [x, y]: [i32; 2], [w, h]: [i32; 2]) -> GuiDraw {
    GuiDraw::Blit {
        texture: texture.into(),
        blend: Blend::Textured,
        x,
        y,
        w,
        h,
        uv: [0.0, 0.0],
        src: [w, h],
        tex: [256, 256],
        color: 0xFFFF_FFFF,
    }
}

// ---------------------------------------------------------------------------
// The mod under test
// ---------------------------------------------------------------------------

/// This mod's screen factories for the given menu types, registered as `content.rs` does.
struct ScreenMod {
    java: Arc<HorizonJava>,
    menus: &'static [&'static str],
}

#[mod_api::__subscriptions]
impl Hooks for ScreenMod {
    fn register(&self, registrar: &mut dyn Registrar, _context: &RegistrationContext) {
        for menu in self.menus {
            crate::client::menu_screen::register_screen(registrar, menu, &self.java.client);
        }
    }
}

/// A client that ran `jar` (its `main` and `client` entrypoints, as client setup does), with
/// this mod's factories installed for `menus`.
fn with_client(jar: Vec<u8>, menus: &'static [&'static str], run: impl FnOnce(&HorizonJava)) {
    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("screens.jar".into(), jar)]);
    java.on_client_setup();
    assert_eq!(java.lock_client().resident_count(), 1, "the jar loaded");
    let mods: mod_api::Mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(ScreenMod {
            java: Arc::clone(&java),
            menus,
        }),
    )]
    .into();
    mod_api::testing::with_mods(mods.clone(), || {
        mod_api::__private::part_tests::install_parts_fixture(&mods).expect("install the parts");
        run(&java);
    });
}

fn counted(java: &HorizonJava) -> Vec<(String, usize)> {
    java.lock_client().counted(0)
}

/// What was counted since `before` (client setup counts the entrypoints' own calls).
fn counted_since(java: &HorizonJava, before: &[(String, usize)]) -> Vec<(String, usize)> {
    let mut added: Vec<(String, usize)> = counted(java)
        .into_iter()
        .filter_map(|(key, count)| {
            let earlier = before
                .iter()
                .find(|(seen, _)| *seen == key)
                .map_or(0, |(_, n)| *n);
            (count > earlier).then(|| (key, count - earlier))
        })
        .collect();
    added.sort();
    added
}

fn on_a_big_stack(name: &str, run: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name(name.into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run)
        .expect("spawn test thread")
        .join()
        .unwrap();
}

// ---------------------------------------------------------------------------
// The javac fixture
// ---------------------------------------------------------------------------

/// The screens: `BenchScreen` is shaped like Runes' `RuneCraftingScreen` (an `ItemCombinerScreen`
/// with its own texture and title position) plus a known `extractErrorIcon` fill; `ShelfScreen`
/// is a plain `AbstractContainerScreen` drawing its own background, and overriding
/// `extractLabels` and `mouseClicked`, which Sunrise's container screen does itself.
const SCREEN_SOURCES: &[(&str, &str)] = &[
    (
        "demo/ScreenProbe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         public final class ScreenProbe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"shelf\"), ShelfMenu.TYPE);\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"cost\"), CostMenu.TYPE);\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"bare\"), BareMenu.TYPE);\n\
           }\n\
         }",
    ),
    (
        "demo/ShelfMenu.java",
        "package demo;\n\
         import net.minecraft.world.SimpleContainer;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.inventory.Slot;\n\
         public class ShelfMenu extends AbstractContainerMenu {\n\
           public static final MenuType<ShelfMenu> TYPE = new MenuType<>(ShelfMenu::new, FeatureFlags.VANILLA_SET);\n\
           public final SimpleContainer box = new SimpleContainer(1);\n\
           public ShelfMenu(int id, Inventory inventory) {\n\
             super(TYPE, id);\n\
             this.addSlot(new IconSlot(this.box, 0, 80, 20));\n\
             this.addStandardInventorySlots(inventory, 8, 51);\n\
           }\n\
         }",
    ),
    (
        "demo/BenchScreen.java",
        "package demo;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.client.gui.screens.inventory.ItemCombinerScreen;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         public class BenchScreen extends ItemCombinerScreen<BenchMenu> {\n\
           private static final Identifier TEXTURE = Identifier.fromNamespaceAndPath(\"demo\", \"textures/gui/bench.png\");\n\
           public BenchScreen(BenchMenu menu, Inventory inventory, Component title) {\n\
             super(menu, inventory, title, TEXTURE);\n\
             this.titleLabelX = 60;\n\
             this.titleLabelY = 18;\n\
           }\n\
           public static int removals;\n\
           @Override protected void extractErrorIcon(GuiGraphicsExtractor graphics, int x, int y) {\n\
             graphics.fill(x + 99, y + 45, x + 127, y + 66, 0xFF8B8B8B);\n\
           }\n\
           @Override public void removed() { super.removed(); removals++; }\n\
         }",
    ),
    (
        "demo/ShelfScreen.java",
        "package demo;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;\n\
         import net.minecraft.client.input.MouseButtonEvent;\n\
         import net.minecraft.client.renderer.RenderPipelines;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         public class ShelfScreen extends AbstractContainerScreen<ShelfMenu> {\n\
           private static final Identifier TEXTURE = Identifier.fromNamespaceAndPath(\"demo\", \"textures/gui/shelf.png\");\n\
           public ShelfScreen(ShelfMenu menu, Inventory inventory, Component title) { super(menu, inventory, Component.translatable(\"container.demo.own_shelf\"), 176, 133); }\n\
           @Override public void extractBackground(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {\n\
             super.extractBackground(graphics, mouseX, mouseY, partialTick);\n\
             graphics.blit(RenderPipelines.GUI_TEXTURED, TEXTURE, this.leftPos, this.topPos, 0.0f, 0.0f, this.imageWidth, this.imageHeight, 256, 256);\n\
             if (this.menu.box.getItem(0).isEmpty()) graphics.fill(this.leftPos, this.topPos, this.leftPos + 4, this.topPos + 4, 0xFFFF0000);\n\
           }\n\
           @Override protected void extractLabels(GuiGraphicsExtractor graphics, int mouseX, int mouseY) {}\n\
           @Override public boolean mouseClicked(MouseButtonEvent event, boolean doubleClick) { return false; }\n\
         }",
    ),
    (
        "demo/IconSlot.java",
        "package demo;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.inventory.Slot;\n\
         public class IconSlot extends Slot {\n\
           public IconSlot(Container container, int index, int x, int y) { super(container, index, x, y); }\n\
           @Override public Identifier getNoItemIcon() { return Identifier.fromNamespaceAndPath(\"demo\", \"container/slot/shelf\"); }\n\
         }",
    ),
    (
        "demo/CostMenu.java",
        "package demo;\n\
         import net.minecraft.world.SimpleContainer;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.DataSlot;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.inventory.Slot;\n\
         public class CostMenu extends AbstractContainerMenu {\n\
           public static final MenuType<CostMenu> TYPE = new MenuType<>(CostMenu::new, FeatureFlags.VANILLA_SET);\n\
           public final DataSlot cost = DataSlot.standalone();\n\
           public CostMenu(int id, Inventory inventory) {\n\
             super(TYPE, id);\n\
             this.addSlot(new Slot(new SimpleContainer(1), 0, 80, 35));\n\
             this.addStandardInventorySlots(inventory, 8, 84);\n\
             this.addDataSlot(this.cost);\n\
           }\n\
         }",
    ),
    (
        "demo/CostScreen.java",
        "package demo;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         public class CostScreen extends AbstractContainerScreen<CostMenu> {\n\
           public CostScreen(CostMenu menu, Inventory inventory, Component title) { super(menu, inventory, title); }\n\
           @Override public void extractBackground(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {\n\
             super.extractBackground(graphics, mouseX, mouseY, partialTick);\n\
             int cost = this.menu.cost.get();\n\
             graphics.fill(this.leftPos + 8, this.topPos + 70, this.leftPos + 8 + cost, this.topPos + 74, 0xFF00FF00);\n\
           }\n\
         }",
    ),
    (
        "demo/BareMenu.java",
        "package demo;\n\
         import net.minecraft.world.SimpleContainer;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.inventory.Slot;\n\
         public class BareMenu extends AbstractContainerMenu {\n\
           public static final MenuType<BareMenu> TYPE = new MenuType<>(BareMenu::new, FeatureFlags.VANILLA_SET);\n\
           public BareMenu(int id, Inventory inventory) {\n\
             super(TYPE, id);\n\
             this.addSlot(new Slot(new SimpleContainer(1), 0, 80, 35));\n\
             this.addStandardInventorySlots(inventory, 8, 84);\n\
           }\n\
         }",
    ),
    (
        "demo/BareScreen.java",
        "package demo;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;\n\
         import net.minecraft.client.renderer.RenderPipelines;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         public class BareScreen extends AbstractContainerScreen<BareMenu> {\n\
           private static final Identifier TEXTURE = Identifier.fromNamespaceAndPath(\"demo\", \"textures/gui/bare.png\");\n\
           public BareScreen(BareMenu menu, Inventory inventory, Component title) { super(menu, inventory, title); }\n\
           @Override protected void init() { super.init(); this.titleLabelX = this.width / 8; }\n\
           @Override public void extractBackground(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {\n\
             graphics.blit(RenderPipelines.GUI_TEXTURED, TEXTURE, this.leftPos, this.topPos, 0.0f, 0.0f, this.imageWidth, this.imageHeight, 256, 256);\n\
           }\n\
         }",
    ),
    (
        "demo/ClientProbe.java",
        "package demo;\n\
         import net.fabricmc.api.ClientModInitializer;\n\
         import net.minecraft.client.gui.screens.MenuScreens;\n\
         public final class ClientProbe implements ClientModInitializer {\n\
           @Override public void onInitializeClient() {\n\
             MenuScreens.register(BenchMenu.TYPE, BenchScreen::new);\n\
             MenuScreens.register(ShelfMenu.TYPE, ShelfScreen::new);\n\
             MenuScreens.register(CostMenu.TYPE, CostScreen::new);\n\
             MenuScreens.register(BareMenu.TYPE, BareScreen::new);\n\
           }\n\
         }",
    ),
];

/// `live_menu_tests`' stubs this fixture needs wider: an `Inventory` that is a `Container`, a
/// menu that adds slots, a `Slot` with a position.
const REPLACED_STUBS: &[&str] = &[
    "net/minecraft/network/chat/Component.java",
    "net/minecraft/world/entity/player/Inventory.java",
    "net/minecraft/world/inventory/AbstractContainerMenu.java",
    "net/minecraft/world/inventory/Slot.java",
];

/// Compile-only 26.3 client signatures (the client jar's), and the widened common stubs.
const SCREEN_STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat;\n\
         public interface Component {\n\
           static MutableComponent translatable(String key) { return null; }\n\
           static MutableComponent literal(String text) { return null; }\n\
         }",
    ),
    (
        "net/fabricmc/api/ClientModInitializer.java",
        "package net.fabricmc.api; public interface ClientModInitializer { void onInitializeClient(); }",
    ),
    (
        "net/minecraft/world/entity/player/Inventory.java",
        "package net.minecraft.world.entity.player;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class Inventory implements net.minecraft.world.Container {\n\
           public ItemStack getItem(int slot) { return null; }\n\
           public void setItem(int slot, ItemStack stack) {}\n\
           public Component getDisplayName() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/SimpleContainer.java",
        "package net.minecraft.world;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class SimpleContainer implements Container {\n\
           public SimpleContainer(int size) {}\n\
           public ItemStack getItem(int slot) { return null; }\n\
           public void setItem(int slot, ItemStack stack) {}\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/Slot.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         public class Slot {\n\
           public final Container container; public final int x, y;\n\
           public Slot(Container container, int index, int x, int y) { this.container = container; this.x = x; this.y = y; }\n\
           public net.minecraft.resources.Identifier getNoItemIcon() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/AbstractContainerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.entity.player.Player;\n\
         public abstract class AbstractContainerMenu {\n\
           protected AbstractContainerMenu(MenuType<?> type, int id) {}\n\
           protected Slot addSlot(Slot slot) { return slot; }\n\
           protected void addStandardInventorySlots(Container container, int x, int y) {}\n\
           protected DataSlot addDataSlot(DataSlot slot) { return slot; }\n\
           public void removed(Player player) {}\n\
           public ItemStack getCarried() { return null; }\n\
           public boolean canTakeItemForPickAll(ItemStack stack, Slot slot) { return true; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ContainerListener.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public interface ContainerListener {\n\
           void slotChanged(AbstractContainerMenu menu, int slot, ItemStack stack);\n\
           void dataChanged(AbstractContainerMenu menu, int id, int value);\n\
         }",
    ),
    (
        "net/minecraft/client/Minecraft.java",
        "package net.minecraft.client; public class Minecraft {}",
    ),
    (
        "net/minecraft/client/gui/Font.java",
        "package net.minecraft.client.gui; public class Font {}",
    ),
    (
        "net/minecraft/client/input/MouseButtonEvent.java",
        "package net.minecraft.client.input; public record MouseButtonEvent(double x, double y, int button) {}",
    ),
    (
        "com/mojang/renderpearl/api/pipeline/RenderPipeline.java",
        "package com.mojang.renderpearl.api.pipeline; public class RenderPipeline {}",
    ),
    (
        "net/minecraft/client/renderer/RenderPipelines.java",
        "package net.minecraft.client.renderer;\n\
         import com.mojang.renderpearl.api.pipeline.RenderPipeline;\n\
         public class RenderPipelines { public static final RenderPipeline GUI_TEXTURED = null; }",
    ),
    (
        "net/minecraft/client/gui/GuiGraphicsExtractor.java",
        "package net.minecraft.client.gui;\n\
         import com.mojang.renderpearl.api.pipeline.RenderPipeline;\n\
         import net.minecraft.resources.Identifier;\n\
         public class GuiGraphicsExtractor {\n\
           public void fill(int x0, int y0, int x1, int y1, int color) {}\n\
           public void blit(RenderPipeline pipeline, Identifier texture, int x, int y, float u, float v, int w, int h, int texW, int texH) {}\n\
         }",
    ),
    (
        "net/minecraft/client/gui/screens/Screen.java",
        "package net.minecraft.client.gui.screens;\n\
         import net.minecraft.client.Minecraft;\n\
         import net.minecraft.client.gui.Font;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.client.input.MouseButtonEvent;\n\
         import net.minecraft.network.chat.Component;\n\
         public abstract class Screen {\n\
           protected final Component title; protected final Minecraft minecraft = null; protected final Font font = null;\n\
           public int width; public int height;\n\
           protected Screen(Component title) { this.title = title; }\n\
           protected void init() {}\n\
           public void removed() {}\n\
           public void extractBackground(GuiGraphicsExtractor g, int mx, int my, float pt) {}\n\
           public boolean mouseClicked(MouseButtonEvent event, boolean doubleClick) { return false; }\n\
         }",
    ),
    (
        "net/minecraft/client/gui/screens/MenuScreens.java",
        "package net.minecraft.client.gui.screens;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         public final class MenuScreens {\n\
           public interface ScreenConstructor<M extends AbstractContainerMenu, S extends Screen> { S create(M menu, Inventory inventory, Component title); }\n\
           public static <M extends AbstractContainerMenu, S extends Screen> void register(MenuType<? extends M> type, ScreenConstructor<M, S> constructor) {}\n\
         }",
    ),
    (
        "net/minecraft/client/gui/screens/inventory/AbstractContainerScreen.java",
        "package net.minecraft.client.gui.screens.inventory;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.client.gui.screens.Screen;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         public abstract class AbstractContainerScreen<T extends AbstractContainerMenu> extends Screen {\n\
           protected final int imageWidth; protected final int imageHeight;\n\
           protected int titleLabelX; protected int titleLabelY; protected int inventoryLabelX; protected int inventoryLabelY;\n\
           protected final T menu; protected int leftPos; protected int topPos;\n\
           protected AbstractContainerScreen(T menu, Inventory inventory, Component title) { this(menu, inventory, title, 176, 166); }\n\
           protected AbstractContainerScreen(T menu, Inventory inventory, Component title, int w, int h) { super(title); this.menu = menu; this.imageWidth = w; this.imageHeight = h; }\n\
           protected void extractLabels(GuiGraphicsExtractor g, int mx, int my) {}\n\
         }",
    ),
    (
        "net/minecraft/client/gui/screens/inventory/ItemCombinerScreen.java",
        "package net.minecraft.client.gui.screens.inventory;\n\
         import net.minecraft.client.gui.GuiGraphicsExtractor;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.ContainerListener;\n\
         import net.minecraft.world.inventory.ItemCombinerMenu;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public abstract class ItemCombinerScreen<T extends ItemCombinerMenu> extends AbstractContainerScreen<T> implements ContainerListener {\n\
           public ItemCombinerScreen(T menu, Inventory inventory, Component title, Identifier menuResource) { super(menu, inventory, title); }\n\
           protected abstract void extractErrorIcon(GuiGraphicsExtractor g, int x, int y);\n\
           public void slotChanged(AbstractContainerMenu menu, int slot, ItemStack stack) {}\n\
           public void dataChanged(AbstractContainerMenu menu, int id, int value) {}\n\
         }",
    ),
];

/// The fixture jar, built with `javac --release 21`, packing only `demo/` classes.
fn screens_jar() -> Vec<u8> {
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "horizon-java-mod-menu-screen-{}-{unique}",
        std::process::id()
    ));
    let _cleanup = Cleanup(root.clone());
    let classes = root.join("classes");
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| PathBuf::from("javac"));
    let mut command = Command::new(javac);
    command
        .args(["--release", "21", "-nowarn", "-d"])
        .arg(&classes);
    let sources = STUBS
        .iter()
        .filter(|(path, _)| !REPLACED_STUBS.contains(path))
        .chain(SCREEN_STUBS)
        .chain(DEMO_SOURCES)
        .chain(SCREEN_SOURCES);
    for (path, source) in sources {
        let file = root.join("src").join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        command.arg(file);
    }
    let output = command.output().expect("run javac");
    assert!(
        output.status.success(),
        "javac failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        let options = SimpleFileOptions::default();
        zip.start_file("fabric.mod.json", options).unwrap();
        zip.write_all(
            br#"{"schemaVersion":1,"id":"demo","version":"1","entrypoints":{"main":["demo.Probe","demo.ScreenProbe"],"client":["demo.ClientProbe"]}}"#,
        )
        .unwrap();
        let mut files: Vec<PathBuf> = fs::read_dir(classes.join("demo"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        files.sort();
        for file in files {
            let name = format!("demo/{}", file.file_name().unwrap().to_string_lossy());
            zip.start_file(name, options).unwrap();
            zip.write_all(&fs::read(&file).unwrap()).unwrap();
        }
        zip.finish().unwrap();
    }
    bytes.into_inner()
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// The `ItemCombinerScreen` opens for its menu type with its own layout, draws its texture (and
/// its `extractErrorIcon`) where `init` put it, follows a resize, leaves clicks to Sunrise's
/// container screen, and runs `removed()` when the window drops it. 0 calls counted except the
/// client inventory the screen frame doesn't fill.
#[test]
fn javac_item_combiner_screen_opens_draws_resizes_and_closes_through_the_sunrise_hooks() {
    on_a_big_stack("menu-screen-bench", || {
        with_client(screens_jar(), &["demo:bench", "demo:shelf"], |java| {
            let before = counted(java);
            let host = ScreenHost::new([640, 480], 2);
            let title = Text::translate("container.demo.bench", Vec::new());
            let (owner, screen) =
                mod_api::__private::create_menu_screen(&host, open(8, "demo:bench", &title, 3))
                    .expect("the menu type has a screen factory");
            assert_eq!(owner, "horizon_java");
            let screen = screen
                .expect("the factory did not panic")
                .unwrap_or_else(|| {
                    panic!("the Java screen opened: {:?}", counted_since(java, &before))
                });
            assert!(java.lock_client().menu_screen_open());

            // The layout the Java screen set: 26.3 `AbstractContainerScreen.<init>` (3-arg) is
            // 176x166 with `inventoryLabelY = 166 - 94`; `BenchScreen.<init>` moves the title to
            // (60, 18); `BenchMenu`'s slots are (27, 47), (76, 47), the result (134, 47), then the
            // standard inventory from (8, 84).
            let layout = screen.layout();
            assert_eq!((layout.width, layout.height), (176.0, 166.0));
            let slots: Vec<[f32; 2]> = layout
                .slots
                .iter()
                .map(|slot| slot.as_ref().expect("visible").position)
                .collect();
            assert_eq!(slots, vec![[27.0, 47.0], [76.0, 47.0], [134.0, 47.0]]);
            assert!(layout.slots.iter().all(|slot| {
                slot.as_ref()
                    .is_some_and(|slot| slot.no_item_icon.is_none())
            }));
            assert_eq!((layout.inventory_x, layout.inventory_y), (8.0, 84.0));
            // The labels Sunrise draws for `extractLabels` (in 0xFF404040, no shadow): the
            // server's title and the inventory's, at the screen's positions.
            assert_eq!(layout.title_label, Some([60.0, 18.0]));
            assert_eq!(layout.inventory_label, Some([8.0, 72.0]));
            assert!(!layout.own_labels);
            // `ItemCombinerScreen.extractBackground` is the whole background.
            assert!(layout.own_background);

            // A frame at 640x480 / 2: `init` put the panel at ((320 - 176) / 2, (240 - 166) / 2).
            let mut painted = Painted::default();
            screen.draw(&host, frame([320, 240], [176, 166]), &mut painted);
            assert_eq!(
                painted.0,
                vec![
                    // ItemCombinerScreen.extractBackground offsets 9-41.
                    blit("demo:gui/bench", [72, 37], [176, 166]),
                    // Then BenchScreen.extractErrorIcon(leftPos, topPos).
                    GuiDraw::Fill {
                        x0: 171,
                        y0: 82,
                        x1: 199,
                        y1: 103,
                        color: 0xFF8B_8B8B,
                    },
                ]
            );

            // A press off the slots: the Java screen doesn't override `mouseClicked`, so it is
            // `AbstractContainerScreen.mouseClicked`'s, which Sunrise's container screen does
            // (slot clicks go to the server's Java menu through #662, outside clicks throw).
            assert_eq!(
                screen.click(&host, frame([320, 240], [176, 166]), false),
                None
            );
            assert_eq!(
                screen.click(&host, frame([320, 240], [176, 166]), true),
                None
            );

            // The window grew: `Screen.resize` → `rebuildWidgets` → `init()` moves the panel.
            let mut painted = Painted::default();
            screen.draw(&host, frame([400, 300], [176, 166]), &mut painted);
            assert_eq!(
                painted.0,
                vec![
                    blit("demo:gui/bench", [112, 67], [176, 166]),
                    GuiDraw::Fill {
                        x0: 211,
                        y0: 112,
                        x1: 239,
                        y1: 133,
                        color: 0xFF8B_8B8B,
                    },
                ]
            );

            // The window drops the opening: `removed()` runs and the screen is gone.
            drop(screen);
            assert!(!java.lock_client().menu_screen_open());
            // `ItemCombinerScreen.init`'s `addSlotListener` broadcast reads stacks the client
            // menu doesn't have, but nothing observes them there (no synchronizer, and
            // `ItemCombinerScreen.slotChanged` is `return`): nothing is counted.
            assert_eq!(counted_since(java, &before), Vec::new());
        });
    });
}

/// A plain `AbstractContainerScreen` that draws its own background: the blit comes out at its
/// `leftPos`/`topPos`; its `extractLabels` and `mouseClicked` overrides, which Sunrise's
/// container screen does instead, are counted once at open, and its `extractBackground`
/// override's partial tick (which Sunrise's frame lacks) is counted per frame.
#[test]
fn javac_container_screen_counts_the_overrides_sunrise_performs() {
    on_a_big_stack("menu-screen-shelf", || {
        with_client(screens_jar(), &["demo:bench", "demo:shelf"], |java| {
            let before = counted(java);
            let host = ScreenHost::new([640, 480], 2);
            let title = Text::literal("Shelf");
            let (_, screen) =
                mod_api::__private::create_menu_screen(&host, open(3, "demo:shelf", &title, 1))
                    .expect("the menu type has a screen factory");
            let screen = screen.unwrap().expect("the Java screen opened");
            let layout = screen.layout();
            assert_eq!((layout.width, layout.height), (176.0, 133.0));
            assert_eq!(
                layout.slots,
                // `IconSlot.getNoItemIcon()` ran at open; Sunrise won't ask it again (counted).
                vec![Some(mod_api::ScreenSlot {
                    position: [80.0, 20.0],
                    no_item_icon: Some("demo:container/slot/shelf".to_owned()),
                })]
            );
            assert_eq!((layout.inventory_x, layout.inventory_y), (8.0, 51.0));
            assert_eq!(layout.title_label, Some([8.0, 6.0]));
            assert_eq!(layout.inventory_label, Some([8.0, 39.0]));

            // The blit, then `menu.box.getItem(0)`: the client box never got the server's
            // contents, so the read is counted and the frame stops there (no `isEmpty` fill).
            let mut painted = Painted::default();
            screen.draw(&host, frame([320, 240], [176, 133]), &mut painted);
            assert_eq!(
                painted.0,
                vec![blit("demo:gui/shelf", [72, 53], [176, 133])]
            );
            drop(screen);

            assert_eq!(
                counted_since(java, &before),
                vec![
                    (
                        "demo/IconSlot#getNoItemIcon()Lnet/minecraft/resources/Identifier;"
                            .to_owned(),
                        1
                    ),
                    (
                        "demo/ShelfScreen#extractLabels(Lnet/minecraft/client/gui/GuiGraphicsExtractor;II)V"
                            .to_owned(),
                        1
                    ),
                    (
                        "demo/ShelfScreen#mouseClicked(Lnet/minecraft/client/input/MouseButtonEvent;Z)Z"
                            .to_owned(),
                        1
                    ),
                    (
                        "game-hook re-entry: sunrise#menu_screen:screen_title_is_not_the_server_title"
                            .to_owned(),
                        1
                    ),
                    (
                        "net/minecraft/client/DeltaTracker#getGameTimeDeltaPartialTick(Z)F"
                            .to_owned(),
                        1
                    ),
                    (
                        "net/minecraft/world/SimpleContainer#getItem(I)Lnet/minecraft/world/item/ItemStack;"
                            .to_owned(),
                        1
                    ),
                ]
            );
        });
    });
}

/// A background drawn from a data slot (`CostScreen` fills `menu.cost.get()` pixels): each
/// frame writes Sunrise's synced value into the client menu first, as vanilla's
/// `ClientboundContainerSetDataPacket` → `setData` does, sign-extended from the packet's
/// `short`. Before Sunrise has a value the slot keeps the value the menu built (0).
#[test]
fn a_data_slot_driven_background_follows_the_servers_values() {
    on_a_big_stack("menu-screen-cost", || {
        with_client(screens_jar(), &["demo:cost"], |java| {
            let before = counted(java);
            let host = ScreenHost::new([640, 480], 2);
            let title = Text::literal("Cost");
            let (_, screen) =
                mod_api::__private::create_menu_screen(&host, open(5, "demo:cost", &title, 1))
                    .expect("the menu type has a screen factory");
            let screen = screen.unwrap().expect("the Java screen opened");
            let bar = |data: &'static [i32]| {
                let mut painted = Painted::default();
                let mut at = frame([320, 240], [176, 166]);
                at.data = data;
                screen.draw(&host, at, &mut painted);
                painted.0
            };
            // leftPos 72, topPos 37: the bar spans x 80 .. 80 + cost, y 107 .. 111.
            let fill = |x1: i32| GuiDraw::Fill {
                x0: 80,
                y0: 107,
                x1,
                y1: 111,
                color: 0xFF00_FF00,
            };
            assert_eq!(bar(&[]), vec![fill(80)]);
            assert_eq!(bar(&[37]), vec![fill(117)]);
            assert_eq!(bar(&[12]), vec![fill(92)]);
            // 70000 travels as the short 4464 (70000 - 65536).
            assert_eq!(bar(&[70000]), vec![fill(80 + 4464)]);
            drop(screen);
            // `CostScreen` overrides `extractBackground`, which gets the partial tick Sunrise's
            // frame lacks: counted on each of the four frames. Nothing else.
            assert_eq!(
                counted_since(java, &before),
                vec![(
                    "net/minecraft/client/DeltaTracker#getGameTimeDeltaPartialTick(Z)F".to_owned(),
                    4
                )]
            );
        });
    });
}

/// A screen whose `extractBackground` never reaches `Screen.extractBackground` has no backdrop
/// in vanilla, but Sunrise draws one under every mod screen: counted per frame. Its `init`
/// puts the title at `width / 8`, so a resize moves the label Sunrise took at open: counted.
#[test]
fn a_background_without_super_and_a_label_moved_by_init_are_counted() {
    on_a_big_stack("menu-screen-bare", || {
        with_client(screens_jar(), &["demo:bare"], |java| {
            let before = counted(java);
            let host = ScreenHost::new([640, 480], 2);
            let title = Text::literal("Bare");
            let (_, screen) =
                mod_api::__private::create_menu_screen(&host, open(6, "demo:bare", &title, 1))
                    .expect("the menu type has a screen factory");
            let screen = screen.unwrap().expect("the Java screen opened");
            assert_eq!(screen.layout().title_label, Some([40.0, 6.0]));
            let mut painted = Painted::default();
            screen.draw(&host, frame([320, 240], [176, 166]), &mut painted);
            assert_eq!(painted.0, vec![blit("demo:gui/bare", [72, 37], [176, 166])]);
            let mut painted = Painted::default();
            screen.draw(&host, frame([400, 300], [176, 166]), &mut painted);
            assert_eq!(
                painted.0,
                vec![blit("demo:gui/bare", [112, 67], [176, 166])]
            );
            drop(screen);
            assert_eq!(
                counted_since(java, &before),
                vec![
                    (
                        "game-hook re-entry: sunrise#menu_screen:backdrop_without_Screen.extractBackground"
                            .to_owned(),
                        2
                    ),
                    (
                        "game-hook re-entry: sunrise#menu_screen:layout_changed_after_open"
                            .to_owned(),
                        1
                    ),
                    (
                        "net/minecraft/client/DeltaTracker#getGameTimeDeltaPartialTick(Z)F"
                            .to_owned(),
                        2
                    ),
                ]
            );
        });
    });
}

/// The world unloads with the screen open: vanilla's `disconnect` / `clearClientLevel` run its
/// `removed()` (`Gui.setScreen`, before `player` is cleared); the next world opens fresh.
#[test]
fn a_world_unload_closes_the_open_screen() {
    on_a_big_stack("menu-screen-unload", || {
        with_client(screens_jar(), &["demo:bench"], |java| {
            let host = ScreenHost::new([640, 480], 2);
            let title = Text::translate("container.demo.bench", Vec::new());
            let open_bench = || {
                mod_api::__private::create_menu_screen(&host, open(9, "demo:bench", &title, 3))
                    .expect("the menu type has a screen factory")
                    .1
                    .unwrap()
                    .expect("the Java screen opened")
            };
            let removals = |java: &HorizonJava| {
                // A static never written still holds its default, 0.
                java.lock_client()
                    .static_value(0, "demo/BenchScreen", "removalsI")
                    .or(Some(crate::runtime::Value::Int(0)))
            };
            let screen = open_bench();
            assert_eq!(removals(java), Some(crate::runtime::Value::Int(0)));
            java.on_client_level_unload();
            assert!(!java.lock_client().menu_screen_open());
            assert_eq!(removals(java), Some(crate::runtime::Value::Int(1)));
            // The window drops the old opening later: nothing runs twice.
            drop(screen);
            assert_eq!(removals(java), Some(crate::runtime::Value::Int(1)));
            java.on_client_level_load();
            let screen = open_bench();
            drop(screen);
            assert_eq!(removals(java), Some(crate::runtime::Value::Int(2)));
        });
    });
}

/// A menu type whose jar registered no `MenuScreens` constructor opens no screen, as vanilla's
/// `MenuScreens.create` (it logs and keeps the current screen), and counts nothing.
#[test]
fn a_menu_type_without_a_java_screen_opens_none() {
    on_a_big_stack("menu-screen-none", || {
        with_client(screens_jar(), &["demo:unscreened"], |java| {
            let before = counted(java);
            let host = ScreenHost::new([640, 480], 2);
            let title = Text::literal("None");
            let (_, screen) = mod_api::__private::create_menu_screen(
                &host,
                open(4, "demo:unscreened", &title, 1),
            )
            .expect("the menu type has a screen factory");
            assert!(screen.unwrap().is_none());
            assert!(!java.lock_client().menu_screen_open());
            assert!(counted_since(java, &before).is_empty());
        });
    });
}

/// Runes 1.3.2: right-clicking the altar opens `runes:crafting` (#662); its client screen is
/// `RuneCraftingScreen`, which this mod now shows: the altar texture at the panel, the title at
/// (60, 18), the two inputs and the result where `RuneCraftingScreenHandler` put them.
#[test]
fn real_runes_altar_screen_opens_and_draws_through_the_sunrise_hooks() {
    let Some(path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes screen test skipped");
        return;
    };
    let bytes = fs::read(&path).expect("read RUNES_JAR");
    on_a_big_stack("menu-screen-runes", move || {
        with_client(bytes, &["runes:crafting"], |java| {
            let before = counted(java);
            let host = ScreenHost::new([640, 480], 2);
            // `RuneCraftingBlock`'s provider title (offset 77): `gui.runes.rune_crafting`.
            let title = Text::translate("gui.runes.rune_crafting", Vec::new());
            let (_, screen) =
                mod_api::__private::create_menu_screen(&host, open(8, "runes:crafting", &title, 3))
                    .expect("the menu type has a screen factory");
            let screen = screen.unwrap().expect("RuneCraftingScreen opened");
            let layout = screen.layout();
            assert_eq!((layout.width, layout.height), (176.0, 166.0));
            let slots: Vec<[f32; 2]> = layout
                .slots
                .iter()
                .map(|slot| slot.as_ref().unwrap().position)
                .collect();
            assert_eq!(slots, vec![[27.0, 47.0], [76.0, 47.0], [134.0, 47.0]]);
            assert_eq!((layout.inventory_x, layout.inventory_y), (8.0, 84.0));
            assert_eq!(layout.title_label, Some([60.0, 18.0]));
            assert_eq!(layout.inventory_label, Some([8.0, 72.0]));

            let mut painted = Painted::default();
            screen.draw(&host, frame([320, 240], [176, 166]), &mut painted);
            // ItemCombinerScreen.extractBackground: the altar texture; RuneCraftingScreen's
            // `extractErrorIcon` is `return`.
            assert_eq!(
                painted.0,
                vec![blit("runes:gui/crafting_altar", [72, 37], [176, 166])]
            );
            assert_eq!(
                screen.click(&host, frame([320, 240], [176, 166]), false),
                None
            );
            drop(screen);
            assert!(!java.lock_client().menu_screen_open());
            assert_eq!(counted_since(java, &before), Vec::new());
        });
    });
}
