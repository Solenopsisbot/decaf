//! The client screen of a Java mod's menu (#609 batch 3, W12).
//!
//! #662 opens a Sunrise mod menu for every `player.openMenu` a jar block makes, and the Java menu
//! runs its clicks on the server. A Sunrise client shows such a menu only through a screen
//! factory, so this module registers one per jar menu type and, when the client opens that
//! menu, runs the screen the jar registered with `MenuScreens.register` (`client_registrations`
//! `menu_screen`) through the runtime's container-screen lifecycle (`screen_models.rs`, #613):
//!
//! - **open** (`MenuScreens.create` → `ScreenConstructor.fromPacket` → `Gui.setScreen`): the
//!   client menu from `MenuType.create(window, inventory)`, the screen from the jar's
//!   constructor with the server's title, then `added()` and `init(guiWidth, guiHeight)`;
//! - **layout**: the screen's `imageWidth`/`imageHeight`, label positions and the menu's
//!   `Slot.x/y` become Sunrise's [`mod_api::MenuScreenLayout`];
//! - **draw**: each frame runs the screen's `extractBackground` (a `Screen.resize` first when the
//!   GUI size changed) and paints its `GuiGraphicsExtractor` ops through `DrawContext::gui_paint`;
//! - **close**: when the window drops the screen, `removed()` (`Gui.setScreen` switching it out).
//!
//! Everything else vanilla's `AbstractContainerScreen` / `ItemCombinerScreen` does is Sunrise's
//! own container screen: the slots, their items and highlight, the carried stack, the title and
//! inventory labels (Sunrise draws both in `0xFF404040` without a shadow at the screen's label
//! positions, as `extractLabels` does), tooltips, every click, drag, key and scroll, and client
//! prediction. The server stays authoritative through #662. A jar override of one of those
//! members would run in vanilla and does not run here: the runtime counts each one at open
//! (`SUNRISE_SCREEN_METHODS`). Nothing here names a mod.
//!
//! Each Java call runs in a *screen frame* (`ClientSnapshot::screen_scope`): only the GUI size
//! and the presence of the player and level are filled, so the runtime answers the graphics
//! surface and refuses (counts) every other client fact a screen might read.

use std::sync::{Arc, Mutex, TryLockError};

use mod_api::client::hud_layout::Blend;
use mod_api::{
    ClientHost, DrawContext, GuiDraw, MenuScreen, MenuScreenFactory, MenuScreenFrame,
    MenuScreenLayout, MenuScreenOpen, ScreenSlot,
};

use super::{LoadedJar, State};
use crate::LOG;
use crate::runtime;
use runtime::client_surface::{ClientLevel, ClientPlayer, ClientSnapshot, ClientText, DrawOp};
use runtime::screen_models::ScreenLayout;

/// The player inventory's standard layout (`AbstractContainerMenu.addStandardInventorySlots`,
/// 26.3 offsets 0-79): three rows of nine from `(x, y)`, 18 apart, then the hotbar 58 below the
/// first row. Sunrise lays a mod screen's player slots out the same way from
/// `MenuScreenLayout::inventory_x/inventory_y`.
const HOTBAR_DROP: i32 = 58;

/// One opened screen: which resident jar runs it, and which opening it is. A window id can be
/// reused, so the serial tells a late callback of a closed opening from the current one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Opening {
    jar: usize,
    serial: u64,
}

/// Register the Java screen factory for jar menu type `menu` (its `MenuScreens.register`
/// constructor is looked up when the client opens the menu).
pub(crate) fn register_screen(
    registrar: &mut dyn mod_api::Registrar,
    menu: &str,
    client: &Arc<Mutex<State>>,
) {
    if let Err(error) =
        registrar.register_menu_screen(menu, Arc::new(JavaScreenFactory::new(Arc::clone(client))))
    {
        log::warn!("{LOG}: no client screen factory for menu {menu}: {error}");
    }
}

/// The screen factory registered for one jar menu type. It holds the client state, so the
/// factory, its screen's callbacks and the close guard reach the resident jars.
pub(crate) struct JavaScreenFactory {
    client: Arc<Mutex<State>>,
}

impl JavaScreenFactory {
    pub(crate) fn new(client: Arc<Mutex<State>>) -> Self {
        Self { client }
    }
}

impl MenuScreenFactory for JavaScreenFactory {
    fn create(&self, host: &dyn ClientHost, open: MenuScreenOpen<'_>) -> Option<MenuScreen> {
        // The factory is called by the window, never inside one of this mod's hooks; a held
        // lock means a re-entrant call, which has no answer here.
        let mut state = match self.client.try_lock() {
            Ok(state) => state,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => {
                log::warn!(
                    "{LOG}: client menu screen {} opened re-entrantly",
                    open.menu
                );
                return None;
            }
        };
        let (opening, layout) = state.open_menu_screen(host, &open)?;
        drop(state);
        Some(java_screen(Arc::clone(&self.client), opening, layout))
    }
}

/// The window-owned screen: its layout, the draw callback, and a guard that closes the Java
/// screen when the window drops the opening.
fn java_screen(
    client: Arc<Mutex<State>>,
    opening: Opening,
    layout: MenuScreenLayout,
) -> MenuScreen {
    let guard = CloseGuard {
        client: Arc::clone(&client),
        opening,
    };
    MenuScreen::new(layout).with_draw(move |host, frame, draw| {
        // The guard lives as long as the callback, which the window owns until it closes.
        let _ = &guard;
        let mut state = match client.try_lock() {
            Ok(state) => state,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        state.draw_menu_screen(host, opening, &frame, draw);
    })
}

/// Runs `removed()` on the Java screen when the window drops its opening (Escape, the server
/// closing the menu, another screen replacing it).
struct CloseGuard {
    client: Arc<Mutex<State>>,
    opening: Opening,
}

impl Drop for CloseGuard {
    fn drop(&mut self) {
        let client = Arc::clone(&self.client);
        let opening = self.opening;
        // Never unwind out of a drop.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let mut state = match client.try_lock() {
                Ok(state) => state,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            state.close_menu_screen(opening);
        }));
    }
}

/// The GUI-scaled window size (`Window.getGuiScaledWidth/Height`: the framebuffer size over the
/// scale, rounded up), which `Gui.setScreen` hands `Screen.init`.
fn gui_size(host: &dyn ClientHost) -> Option<[i32; 2]> {
    let [w, h] = host.window_size()?;
    let scale = host.gui_scale()?.max(1);
    Some([
        i32::try_from(w.div_ceil(scale)).ok()?,
        i32::try_from(h.div_ceil(scale)).ok()?,
    ])
}

/// A screen frame: the GUI size, and a player and level that are present (the client has both
/// while a menu is open) but carry no facts; `screen_scope` makes every other read count.
fn screen_frame(gui_size: [i32; 2], player: Option<u128>) -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        player: Some(ClientPlayer {
            uuid: player.unwrap_or_default(),
            ..ClientPlayer::default()
        }),
        level: Some(ClientLevel::default()),
        gui_size,
        screen_scope: true,
        ..ClientSnapshot::default()
    })
}

impl State {
    /// `MenuScreens.create` for an opening of `open.menu`: the resident jar whose
    /// `MenuScreens.register` covers that menu type builds and initializes its screen. `None`
    /// when no jar registered one (vanilla logs and opens nothing, and so does Sunrise) or the
    /// screen could not be built (counted on the jar).
    fn open_menu_screen(
        &mut self,
        host: &dyn ClientHost,
        open: &MenuScreenOpen<'_>,
    ) -> Option<(Opening, MenuScreenLayout)> {
        let found = self.jars.iter().enumerate().find_map(|(index, loaded)| {
            let entry = loaded
                .host
                .observed_registry_entry("minecraft:menu", open.menu)?;
            loaded
                .host
                .client_menu_screen_registered(&entry.value)
                .then(|| (index, entry.value.clone()))
        });
        let (index, menu_type) = found?;
        // `Gui.setScreen` switches the previous screen out first; another jar's screen still
        // open is closed on its own host.
        if let Some(previous) = self.screen.take()
            && previous.jar != index
            && let Some(loaded) = self.jars.get_mut(previous.jar)
        {
            close_on(loaded);
        }
        let player = host.local_player().map(|player| player.uuid());
        let loaded = &mut self.jars[index];
        let Some(size) = gui_size(host) else {
            refuse(loaded, open.menu, "the client has no GUI size");
            return None;
        };
        let Some(title) = title_value(host, open.title) else {
            refuse(
                loaded,
                open.menu,
                "the menu's title has no Java component form",
            );
            return None;
        };
        loaded
            .host
            .begin_client_frame(screen_frame(size, player), &[]);
        let built = build_screen(loaded, open, menu_type, title);
        loaded.host.end_client_frame();
        loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
        let (layout, java_layout) = match built {
            Ok(layout) => layout,
            Err(reason) => {
                log::warn!(
                    "{LOG}: client {} menu screen {} not opened: {reason}",
                    loaded.row.id,
                    open.menu
                );
                return None;
            }
        };
        self.screen_serial += 1;
        let opening = Opening {
            jar: index,
            serial: self.screen_serial,
        };
        self.screen = Some(opening);
        self.screen_layout = Some(java_layout);
        Some((opening, layout))
    }

    /// One frame of an open screen's own drawing, painted through `draw`.
    fn draw_menu_screen(
        &mut self,
        host: &dyn ClientHost,
        opening: Opening,
        frame: &MenuScreenFrame<'_>,
        draw: &mut dyn DrawContext,
    ) {
        if self.screen != Some(opening) {
            return;
        }
        let Some(loaded) = self.jars.get_mut(opening.jar) else {
            return;
        };
        let player = host.local_player().map(|player| player.uuid());
        loaded
            .host
            .begin_client_frame(screen_frame(frame.gui_size, player), &[]);
        // The server's data-slot values Sunrise holds, into the client menu before the frame,
        // as vanilla's `ClientboundContainerSetDataPacket` lands before the render.
        if let Err(error) = loaded
            .jar
            .sync_client_menu_data(&mut loaded.host, frame.data)
        {
            log::debug!("{LOG}: client {} menu data: {error}", loaded.row.id);
        }
        // Sunrise's frame carries no partial tick: a jar `extractBackground` counts it.
        let (ops, result) =
            loaded
                .jar
                .draw_active_client_screen(&mut loaded.host, frame.pointer, None);
        // Sunrise draws the in-game backdrop under every mod screen; vanilla draws it only when
        // the screen's `extractBackground` chain reaches `Screen.extractBackground`. Decided
        // whatever the frame's result: a chain stopped by a counted call before `super` never
        // reached it either.
        if !loaded.host.client_screen_backdrop_ran() {
            loaded.host.note_unknown_game_hook(
                "sunrise#menu_screen:backdrop_without_Screen.extractBackground",
            );
        }
        // The Java screen draws at its own `leftPos`/`topPos`; Sunrise puts its slots and
        // labels at `frame.origin`. They agree unless Sunrise moved the panel (a recipe book,
        // another mod's `screen_room`), which the Java screen never hears of.
        let current = loaded.jar.active_client_screen_layout(&loaded.host);
        if let Some(layout) = &current
            && [layout.left_pos as f32, layout.top_pos as f32] != frame.origin
        {
            loaded
                .host
                .note_unknown_game_hook("sunrise#menu_screen:origin_differs_from_leftPos_topPos");
        }
        // Everything else Sunrise took at open: a later `init` (a resize) that moves a label
        // or changes the image is not on Sunrise's window. `Slot.x/y` are `final` in 26.3.
        if let (Some(now), Some(then)) = (&current, &self.screen_layout)
            && !same_shape(now, then)
        {
            loaded
                .host
                .note_unknown_game_hook("sunrise#menu_screen:layout_changed_after_open");
        }
        let mut draws = Vec::with_capacity(ops.len());
        for op in &ops {
            match gui_draw(host, op) {
                Ok(gui) => draws.push(gui),
                Err(reason) => loaded
                    .host
                    .note_unknown_game_hook(&format!("sunrise#menu_screen:undrawable_{reason}")),
            }
        }
        if !draws.is_empty() && draw.gui_paint(&draws).is_err() {
            loaded
                .host
                .note_unknown_game_hook("sunrise#menu_screen:gui_paint_refused");
        }
        loaded.host.end_client_frame();
        if let Err(error) = result {
            log::debug!("{LOG}: client {} menu screen draw: {error}", loaded.row.id);
        }
        loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
    }

    /// The window dropped `opening`: `removed()` on its Java screen, unless a later opening
    /// already replaced it.
    fn close_menu_screen(&mut self, opening: Opening) {
        if self.screen != Some(opening) {
            return;
        }
        self.screen = None;
        if let Some(loaded) = self.jars.get_mut(opening.jar) {
            close_on(loaded);
        }
    }

    /// The world goes away with a screen open: vanilla's `Minecraft.disconnect` /
    /// `clearClientLevel` switch it out with `Gui.setScreen` (offsets 136/186, 58) while
    /// `player` is still set (cleared at 202, 83), so its `removed()` runs here, before the
    /// world's objects are forgotten. The window's later drop of the opening then does nothing.
    pub(super) fn forget_menu_screen(&mut self) {
        self.screen_layout = None;
        if let Some(opening) = self.screen.take()
            && let Some(loaded) = self.jars.get_mut(opening.jar)
        {
            close_on(loaded);
        }
    }

    /// Whether a Java screen is open on a Sunrise window.
    #[cfg(test)]
    pub(crate) fn menu_screen_open(&self) -> bool {
        self.screen.is_some()
    }

    /// A static field of resident jar `index`'s classes (`owner`, `name` + descriptor).
    #[cfg(test)]
    pub(crate) fn static_value(
        &self,
        index: usize,
        owner: &str,
        key: &str,
    ) -> Option<runtime::Value> {
        runtime::Host::persistent_class_state(&self.jars[index].host)
            .0
            .remove(&format!("{owner}#{key}"))
    }

    /// The counted calls of resident jar `index`, by name.
    #[cfg(test)]
    pub(crate) fn counted(&self, index: usize) -> Vec<(String, usize)> {
        self.jars[index].host.unknown_host_calls()
    }
}

/// `removed()` on the jar's active screen, in a screen frame of its last size.
fn close_on(loaded: &mut LoadedJar) {
    let size = loaded.host.client_active_gui_size().unwrap_or_default();
    loaded
        .host
        .begin_client_frame(screen_frame(size, None), &[]);
    if let Err(error) = loaded.jar.close_active_client_screen(&mut loaded.host) {
        log::debug!("{LOG}: client {} menu screen close: {error}", loaded.row.id);
    }
    loaded.host.end_client_frame();
    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
}

/// Count a screen this bridge could not open, on the jar that registered it.
fn refuse(loaded: &mut LoadedJar, menu: &str, reason: &str) {
    log::warn!(
        "{LOG}: client {} menu screen {menu} not opened: {reason}",
        loaded.row.id
    );
    loaded
        .host
        .note_unknown_game_hook(&format!("sunrise#menu_screen:{reason}"));
    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
}

/// The server's title as the Java `Component` the screen constructor takes, in vanilla's
/// component JSON: a plain literal or a plain translation directly (`{"text"}`,
/// `{"translate"}`), anything styled or nested through its NBT form (`text_to_nbt`, the
/// codec's own). `None` when Sunrise gave none or it has no JSON form here.
fn title_value(host: &dyn ClientHost, title: Option<&mod_api::Text>) -> Option<runtime::Value> {
    use mod_api::text::TextContent;
    let title = title?;
    let json = match &title.content {
        TextContent::Literal(text) if *title == mod_api::Text::literal(text.clone()) => {
            serde_json::json!({ "text": text }).to_string()
        }
        TextContent::Translatable { key, .. }
            if *title == mod_api::Text::translate(key.clone(), Vec::new()) =>
        {
            serde_json::json!({ "translate": key }).to_string()
        }
        _ => super::nbt_component_to_json(&host.text_to_nbt(title).ok()?)?,
    };
    runtime::screen_models::component_from_json(&json)
}

/// Construct, initialize and lay out the jar's screen in the current screen frame.
fn build_screen(
    loaded: &mut LoadedJar,
    open: &MenuScreenOpen<'_>,
    menu_type: runtime::Value,
    title: runtime::Value,
) -> Result<(MenuScreenLayout, ScreenLayout), String> {
    let note = |loaded: &mut LoadedJar, reason: &str| {
        loaded
            .host
            .note_unknown_game_hook(&format!("sunrise#menu_screen:{reason}"));
    };
    if let Err(error) =
        loaded
            .jar
            .create_client_menu(&mut loaded.host, menu_type.clone(), open.window)
    {
        note(loaded, "client_menu_not_built");
        return Err(format!("MenuType.create failed: {error}"));
    }
    let layout = match loaded
        .jar
        .open_client_screen(&mut loaded.host, menu_type, title)
    {
        Ok(layout) => layout.layout,
        Err(error) => {
            note(loaded, "screen_not_built");
            return Err(format!("the screen did not open: {error}"));
        }
    };
    if let Err(error) = loaded.jar.count_sunrise_screen_overrides(&mut loaded.host) {
        log::debug!("{LOG}: client {} screen overrides: {error}", loaded.row.id);
    }
    // Sunrise labels the window with the server's title; vanilla's `extractLabels` draws the
    // screen's own `title`, which a constructor may have replaced.
    if loaded
        .jar
        .active_client_screen_keeps_its_title(&mut loaded.host)
        != Some(true)
    {
        note(loaded, "screen_title_is_not_the_server_title");
    }
    let presentation = match loaded.jar.client_screen_slot_presentation(&mut loaded.host) {
        Ok(presentation) => presentation,
        Err(error) => {
            note(loaded, "slot_presentation_not_read");
            return Err(format!("the slots' presentation: {error}"));
        }
    };
    let (screen_layout, mismatches) = sunrise_layout(&layout, &presentation, open);
    for reason in mismatches {
        note(loaded, reason);
    }
    let java_layout = layout.clone();
    screen_layout
        .map(|screen| (screen, java_layout))
        .ok_or_else(|| {
            note(loaded, "slot_count_mismatch");
            format!(
                "the Java menu has {} slots, Sunrise's window {} + {}",
                layout.slots.len(),
                open.backing_slots,
                open.player_slots
            )
        })
}

/// Sunrise's layout for the Java screen's, and the differences Sunrise would draw (each
/// counted). `None` when the slot counts disagree: Java's slot `i` is Sunrise's slot `i` only
/// when both menus have the same slots (#662 opens the window from the same layout).
pub(crate) fn sunrise_layout(
    layout: &ScreenLayout,
    presentation: &[(bool, Option<String>)],
    open: &MenuScreenOpen<'_>,
) -> (Option<MenuScreenLayout>, Vec<&'static str>) {
    let mut mismatches = Vec::new();
    let backing = open.backing_slots;
    if layout.slots.len() != backing + open.player_slots
        || layout
            .slots
            .iter()
            .enumerate()
            .any(|(at, slot)| usize::try_from(slot.index).ok() != Some(at))
    {
        return (None, mismatches);
    }
    if presentation.len() != layout.slots.len() {
        return (None, mismatches);
    }
    // `Slot.isActive()` false starts the slot hidden (`None`); `getNoItemIcon()` is its empty
    // sprite. Player slots are Sunrise's grid, so a hidden or iconed one there is counted.
    let slots = layout.slots[..backing]
        .iter()
        .zip(presentation)
        .map(|(slot, (active, icon))| {
            active.then(|| ScreenSlot {
                position: [slot.x as f32, slot.y as f32],
                no_item_icon: icon.clone(),
            })
        })
        .collect();
    if presentation[backing..]
        .iter()
        .any(|(active, icon)| !active || icon.is_some())
    {
        mismatches.push("player_slot_hidden_or_iconed");
    }
    let player = &layout.slots[backing..];
    let (inventory_x, inventory_y) = match player.first() {
        Some(first) => (first.x, first.y),
        // No player slots: Sunrise's defaults, which nothing reads.
        None => (8, 84),
    };
    if !player.is_empty() {
        let standard = player.len() == 36
            && player.iter().enumerate().all(|(at, slot)| {
                let (col, row) = ((at % 9) as i32, (at / 9) as i32);
                let y = if row < 3 {
                    inventory_y + row * 18
                } else {
                    inventory_y + HOTBAR_DROP
                };
                (slot.x, slot.y) == (inventory_x + col * 18, y)
            });
        if !standard {
            // Sunrise draws the player's slots on the standard grid from the first one.
            mismatches.push("player_slots_off_the_standard_grid");
        }
    } else {
        // Vanilla's `extractLabels` draws the inventory label whatever the menu holds;
        // Sunrise draws it only with player slots.
        mismatches.push("inventory_label_without_player_slots");
    }
    let screen = MenuScreenLayout {
        width: layout.image_width as f32,
        height: layout.image_height as f32,
        slots,
        inventory_x: inventory_x as f32,
        inventory_y: inventory_y as f32,
        title_label: Some([layout.title_label[0] as f32, layout.title_label[1] as f32]),
        inventory_label: Some([
            layout.inventory_label[0] as f32,
            layout.inventory_label[1] as f32,
        ]),
        // The background is the Java screen's own `extractBackground`; vanilla draws no panel
        // or slot frames of its own.
        own_background: true,
        own_labels: false,
        ..MenuScreenLayout::default()
    };
    (Some(screen), mismatches)
}

/// A whole GUI pixel, or `None` for a fraction (Sunrise's ops take integers).
fn pixel(value: f32) -> Option<i32> {
    (value.is_finite() && value.fract() == 0.0 && value.abs() <= i32::MAX as f32)
        .then_some(value as i32)
}

/// A blit texture's atlas name: `ns:textures/<path>.png` is packed as `ns:<path>`, and
/// vanilla's own as the bare `<path>` (Sunrise's HUD atlas, `atlas_pack.rs`).
fn blit_texture(texture: &str) -> Option<String> {
    let (namespace, path) = texture.split_once(':').unwrap_or(("minecraft", texture));
    let path = path.strip_prefix("textures/")?.strip_suffix(".png")?;
    Some(if namespace == "minecraft" {
        path.to_owned()
    } else {
        format!("{namespace}:{path}")
    })
}

/// One recorded `GuiGraphicsExtractor` op as Sunrise's GUI op, or why it has none.
pub(crate) fn gui_draw(host: &dyn ClientHost, op: &DrawOp) -> Result<GuiDraw, &'static str> {
    let rect = |[x, y, w, h]: [f32; 4]| -> Option<[i32; 4]> {
        Some([pixel(x)?, pixel(y)?, pixel(w)?, pixel(h)?])
    };
    Ok(match op {
        DrawOp::Blit {
            texture,
            src: [u, v, region_w, region_h],
            dst,
            tex_size,
            tint,
        } => {
            let [x, y, w, h] = rect(*dst).ok_or("blit_off_pixel")?;
            GuiDraw::Blit {
                texture: blit_texture(texture).ok_or("blit_texture_path")?.into(),
                blend: Blend::Textured,
                x,
                y,
                w,
                h,
                uv: [*u, *v],
                src: [
                    pixel(*region_w).ok_or("blit_off_pixel")?,
                    pixel(*region_h).ok_or("blit_off_pixel")?,
                ],
                tex: *tex_size,
                color: *tint,
            }
        }
        DrawOp::Fill { rect: r, argb } => {
            let [x, y, w, h] = rect(*r).ok_or("fill_off_pixel")?;
            GuiDraw::Fill {
                x0: x,
                y0: y,
                x1: x + w,
                y1: y + h,
                color: *argb,
            }
        }
        DrawOp::Gradient {
            rect: r,
            top,
            bottom,
        } => {
            let [x, y, w, h] = rect(*r).ok_or("gradient_off_pixel")?;
            GuiDraw::FillGradient {
                x0: x,
                y0: y,
                x1: x + w,
                y1: y + h,
                top: *top,
                bottom: *bottom,
            }
        }
        DrawOp::Sprite {
            sprite,
            rect: r,
            argb,
        } => {
            let [x, y, w, h] = rect(*r).ok_or("sprite_off_pixel")?;
            GuiDraw::Sprite {
                sprite: sprite.clone().into(),
                x,
                y,
                w,
                h,
                region: None,
                color: *argb,
                blend: Blend::Textured,
            }
        }
        DrawOp::Text {
            text,
            at: [x, y],
            argb,
            shadow,
            scale,
        } => {
            if *scale != [1.0, 1.0] {
                return Err("text_scaled");
            }
            GuiDraw::Text {
                text: api_text(host, text).ok_or("text_form")?,
                x: pixel(*x).ok_or("text_off_pixel")?,
                y: pixel(*y).ok_or("text_off_pixel")?,
                color: *argb,
                shadow: *shadow,
            }
        }
        DrawOp::Item { .. } => return Err("item"),
    })
}

/// A drawn text as Sunrise's component: a literal without legacy `§` codes, a bare
/// translation key, or a component's JSON.
fn api_text(host: &dyn ClientHost, text: &ClientText) -> Option<mod_api::Text> {
    match text {
        ClientText::Literal(literal) if !literal.contains('§') => {
            Some(mod_api::Text::literal(literal.clone()))
        }
        ClientText::Literal(_) => None,
        ClientText::Translatable(key) => Some(mod_api::Text::translate(key.clone(), Vec::new())),
        ClientText::Json(json) => host.text_from_json(json).ok().flatten(),
    }
}

/// The parts of the Java layout Sunrise's window took at open (all but the position, which
/// each frame checks against Sunrise's origin).
fn same_shape(now: &ScreenLayout, then: &ScreenLayout) -> bool {
    (
        now.image_width,
        now.image_height,
        now.title_label,
        now.inventory_label,
    ) == (
        then.image_width,
        then.image_height,
        then.title_label,
        then.inventory_label,
    ) && now.slots == then.slots
}
