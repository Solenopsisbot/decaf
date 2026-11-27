//! The client surface a Java mod's client code sees: key mappings, HUD
//! elements, and the `Minecraft` stand-in its tick and connection listeners
//! read.
//!
//! A client entrypoint registers three kinds of things the window drives:
//!
//! - **Key mappings.** `KeyMappingHelper.registerKeyMapping(new KeyMapping(
//!   name, type, key, category))` (Fabric) or `RegisterKeyMappingsEvent
//!   .register(mapping)` (NeoForge). The host records the constructor's
//!   arguments against the object marker and the registration in order; the
//!   game lists them in Controls and answers `consumeClick`/`isDown` on that
//!   marker from real input ([`KeyPress`]).
//! - **HUD elements.** `HudElementRegistry.addLast(id, element)` (Fabric) or
//!   `RegisterGuiLayersEvent.registerAboveAll(id, layer)` (NeoForge). Each
//!   becomes a [`HudLayer`] the game runs every frame
//!   ([`NativeJar::run_hud_layer`]) against a bounded `GuiGraphicsExtractor`
//!   façade that records [`DrawOp`]s: `text`, `centeredText`, `fill`,
//!   `fillGradient`, `outline`, the pose's `translate`/`scale`, and
//!   `Font.width`.
//! - **Client events** (tick, lifecycle, join/disconnect) are ordinary
//!   [`crate::events::GameListener`]s with client [`GameEvent`]s; their
//!   `Minecraft`, `ClientLevel` and `LocalPlayer` arguments are the markers
//!   below.
//!
//! # The stand-ins
//!
//! Every client object is a marker under `client/`: [`MINECRAFT`],
//! [`LEVEL`], [`PLAYER`], the options, the GUI and its chat, the font, the
//! window, the graphics and its pose, the delta tracker. During a firing the
//! game hands a [`ClientSnapshot`] (player, level, GUI size, held vanilla
//! keys, text metrics) and the [`KeyPress`] state of the jar's mappings
//! ([`StdHost::begin_client_frame`]); the markers answer from it, and what
//! the callback did (draws, chat lines, consumed clicks) comes back from
//! [`StdHost::end_client_frame`]. A member the surface does not model fails
//! the callback with [`NativeError::UnknownHostCall`] naming it, so the game
//! disables that callback instead of running it on invented state. Outside a
//! firing (an entrypoint caching `Minecraft.getInstance()`), an unmodelled
//! member falls through to the host's usual lenient answer, exactly as it did
//! before this surface existed; `player` and `level` read `null` then, which
//! is true while the client is starting.
//!
//! Chat components are host handles of their vanilla JSON (`host_component.rs`); a
//! style object (`withStyle(Style)`, `Style`) is kept as legacy codes (`client_text.rs`).

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::{
    ExecutionBudget, HostKind, HostRef, MethodRef, NativeError, NativeJar, StdHost, Value,
    boolean_value, component_value,
    events::{self, GameEvent, ListenerCallback},
    expect_ref,
    host_ref::{ClassHierarchy, ClassSide, Methods, exact_class_rules_out, instance_of_class},
    identifier_parts, return_desc, uuid_value,
};

// The player's items and effects (`ItemStack`, `Inventory`, effects). A
// submodule so the crate root's module list stays the interpreter's.
#[path = "client_items.rs"]
mod items;
pub(crate) use items::NON_NULL_LIST;
pub(crate) use items::inventory_stack_marker;
pub use items::{ClientEffect, ClientItem, ClientStack, ItemTable, StackSlot};
// Style objects (`Style`, `TextColor`, `ChatFormatting`).
#[path = "client_text.rs"]
pub(super) mod text;
// Config libraries' defaults (`AutoConfig`, YACL), for the interpreter's
// library dispatch.
#[path = "client_registrations.rs"]
mod client_registrations;
pub(crate) use client_registrations::range_select_properties;
#[path = "client_config.rs"]
mod config;
// `Font.width` / `Font.split`: the glyph model and the embedder's answers.
#[path = "client_font.rs"]
mod font;
pub use font::{FontAnswers, FontQuestion, WidthOrder};

/// `Minecraft.getInstance()`, and every client listener's `Minecraft`.
pub const MINECRAFT: &str = "client/minecraft";
/// `Minecraft.level`, a `ClientLevel`.
pub const LEVEL: &str = "client/level";
/// `Minecraft.player`, the `LocalPlayer`.
pub const PLAYER: &str = "client/player";
/// A join/disconnect listener's `ClientPacketListener`.
pub const CONNECTION: &str = "client/connection";
/// A join listener's `PacketSender`.
pub const PACKET_SENDER: &str = "client/packet-sender";
const OPTIONS: &str = "client/options";
const GUI: &str = "client/gui";
/// `gui.hud` (26.3 moved the action bar there).
const HUD: &str = "client/hud";
const CHAT: &str = "client/chat";
const FONT: &str = "client/font";
const WINDOW: &str = "client/window";
const GRAPHICS: &str = "client/gui-graphics";
const POSE: &str = "client/pose";
const DELTA: &str = "client/delta-tracker";
/// `Minecraft.gameMode`, the `MultiPlayerGameMode`.
const GAME_MODE: &str = "client/game-mode";
/// `Minecraft.getDebugOverlay()`.
const DEBUG: &str = "client/debug-overlay";
const GAME_TYPE: &str = "net/minecraft/world/level/GameType";
/// `Minecraft.gameRenderer`.
const GAME_RENDERER: &str = "client/game-renderer";
/// `gameRenderer.getMainCamera()`.
const CAMERA: &str = "client/camera";
/// `Minecraft.getTextureManager()`: a bounded stand-in that names textures
/// but never uploads one.
const TEXTURE_MANAGER: &str = "client/texture-manager";
/// `Minecraft.getSingleplayerServer()` in single player. The server runs on
/// its own thread; the window's callbacks see only that it exists.
const INTEGRATED_SERVER: &str = "client/integrated-server";
/// `Minecraft.getUser()`.
const USER: &str = "client/user";
/// `options.<name>()`: `client/option/<name>`.
const OPTION: &str = "client/option/";
/// A `Vec3` the façade handed out: `client/vec3/<x>,<y>,<z>`.
use crate::client_values::VEC3;
/// A tooltip listener's `List<Component>`: the host collection the game
/// seeds with the tooltip's lines and reads back after the firing.
const TOOLTIP_LIST: &str = "java/util/ArrayList#client-tooltip";
const TOOLTIP_CONTEXT_CLASS: &str = "net/minecraft/world/item/Item$TooltipContext";
const TOOLTIP_DISPLAY_CLASS: &str = "net/minecraft/world/item/component/TooltipDisplay";
const TOOLTIP_FLAG_CLASS: &str = "net/minecraft/world/item/TooltipFlag";
const TOOLTIP_FLAG_DEFAULT_CLASS: &str = "net/minecraft/world/item/TooltipFlag$Default";
/// Lines a tooltip listener may leave; more are cut.
const MAX_TOOLTIP_LINES: usize = 64;
static NEXT_TOOLTIP_FLAG_DEFAULT: AtomicU64 = AtomicU64::new(0);

/// A `HostKind::GameTransient` handle for the tooltip's `ItemStack`.
pub fn tooltip_stack() -> Value {
    marker(format!("{}tooltip", items::STACK))
}

/// Dedicated host handle for a level/frame-backed
/// `net.minecraft.world.item.Item$TooltipContext`. `tickRate()` reads the
/// active client frame snapshot's `tick_rate` when supplied and counts
/// otherwise; all other methods (including `Object` methods) are counted.
pub fn tooltip_context() -> Value {
    Value::Host(HostRef::new(HostKind::TooltipContext, "frame"))
}

/// Dedicated host handle for `net.minecraft.world.item.Item$TooltipContext.EMPTY`
/// (`Item$TooltipContext$1` in 26.3: `tickRate() = 20.0f`, `registries() = null`,
/// `mapData(MapId) = null`, `isPeaceful() = false`).
pub fn tooltip_context_empty() -> Value {
    Value::Host(HostRef::new(HostKind::TooltipContext, "empty"))
}

/// Dedicated host handle for `net.minecraft.world.item.component.TooltipDisplay`.
/// All methods (including `Object` methods) are counted.
pub fn tooltip_display() -> Value {
    Value::Host(HostRef::new(HostKind::TooltipDisplay, "default"))
}

/// Constant `TooltipFlag$Default` singleton (`TooltipFlag.NORMAL` / `ADVANCED`).
fn tooltip_flag_default_const(advanced: bool, creative: bool) -> Value {
    Value::Host(HostRef::new(
        HostKind::TooltipFlagDefault,
        format!("const:{},{}", u8::from(advanced), u8::from(creative)),
    ))
}

/// Fresh `new TooltipFlag$Default(advanced, creative)` allocation (`asCreative()`).
fn tooltip_flag_default_new(advanced: bool, creative: bool) -> Value {
    let serial = NEXT_TOOLTIP_FLAG_DEFAULT.fetch_add(1, Ordering::Relaxed);
    Value::Host(HostRef::new(
        HostKind::TooltipFlagDefault,
        format!(
            "inst:{},{}#{serial}",
            u8::from(advanced),
            u8::from(creative)
        ),
    ))
}

/// Dedicated `TooltipFlag$Default` handle with explicit `isAdvanced()` /
/// `advanced()` and `isCreative()` / `creative()` answers. Matches vanilla
/// `TooltipFlag.NORMAL` for `(false, false)`, `TooltipFlag.ADVANCED` for
/// `(true, false)`, and a fresh `TooltipFlag$Default(advanced, true)` for
/// creative tooltips (`asCreative()`).
pub fn tooltip_flag(advanced: bool, creative: bool) -> Value {
    if !creative {
        tooltip_flag_default_const(advanced, false)
    } else {
        tooltip_flag_default_new(advanced, true)
    }
}

/// Opaque frame-backed `TooltipFlag` handle for callbacks invoked without a
/// pre-built `TooltipFlag$Default`; reads flags from the active
/// [`ClientSnapshot`] and counts `Object` methods.
pub(crate) fn tooltip_flag_from_frame() -> Value {
    Value::Host(HostRef::new(HostKind::TooltipFlag, "frame"))
}

fn parse_tooltip_flag_default(key: &str) -> Option<(bool, bool)> {
    let rest = if let Some(rest) = key.strip_prefix("const:") {
        rest
    } else {
        key.strip_prefix("inst:")?.split_once('#')?.0
    };
    let (adv, cre) = rest.split_once(',')?;
    let adv = match adv {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let cre = match cre {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    Some((adv, cre))
}

pub(crate) fn tooltip_consumer_types(_: &str) -> &'static [&'static str] {
    &["java/util/function/Consumer"]
}

pub(crate) fn tooltip_context_types(_: &str) -> &'static [&'static str] {
    &[TOOLTIP_CONTEXT_CLASS]
}

pub(crate) fn tooltip_context_canonical(key: &str) -> bool {
    key == "empty"
}

pub(crate) fn tooltip_display_types(_: &str) -> &'static [&'static str] {
    &[TOOLTIP_DISPLAY_CLASS, "java/lang/Record"]
}

pub(crate) fn tooltip_flag_types(_: &str) -> &'static [&'static str] {
    &[TOOLTIP_FLAG_CLASS]
}

pub(crate) fn tooltip_flag_default_class(_: &str) -> &str {
    TOOLTIP_FLAG_DEFAULT_CLASS
}

pub(crate) fn tooltip_flag_default_canonical(key: &str) -> bool {
    key.starts_with("const:")
}

fn tooltip_display_instance_of(
    _: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if matches!(
        wanted,
        "java/lang/Object" | "java/lang/Record" | TOOLTIP_DISPLAY_CLASS
    ) || hierarchy.is_instance(TOOLTIP_DISPLAY_CLASS, wanted) == Some(true)
    {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false)
        || exact_class_rules_out(TOOLTIP_DISPLAY_CLASS, wanted)
        || wanted == TOOLTIP_FLAG_DEFAULT_CLASS)
        .then_some(false)
}

fn tooltip_flag_default_class_instance_of(
    class: &str,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if matches!(
        wanted,
        "java/lang/Object" | "java/lang/Record" | TOOLTIP_FLAG_CLASS | TOOLTIP_FLAG_DEFAULT_CLASS
    ) || hierarchy.is_instance(class, wanted) == Some(true)
    {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false)
        || exact_class_rules_out(class, wanted)
        || wanted == TOOLTIP_DISPLAY_CLASS)
        .then_some(false)
}

pub(crate) const TOOLTIP_FLAG_DEFAULT_CLASS_SIDE: ClassSide = ClassSide {
    owns: |class| class == TOOLTIP_FLAG_DEFAULT_CLASS,
    instance_of: tooltip_flag_default_class_instance_of,
    superclass: |_| Some("java/lang/Record".to_owned()),
    is_interface: |_| Some(false),
    classes: || vec![TOOLTIP_FLAG_DEFAULT_CLASS.to_owned()],
};

/// Opaque `Consumer<Component>` sink methods: `Object` methods are counted;
/// `instanceof` is exact (`Consumer` and `Object` only, matching vanilla's
/// `lines::add` invokedynamic lambda).
pub(crate) const TOOLTIP_CONSUMER_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |_, _, _| Ok(None),
    instance_of: |_, wanted, _| {
        Some(matches!(
            wanted,
            "java/lang/Object" | "java/util/function/Consumer"
        ))
    },
    crosses_jars: |_, _| false,
    compare_to: None,
};

/// `Item$TooltipContext` handle methods: `toString`/`hashCode`/`getClass` are
/// counted; `EMPTY` (`Item$TooltipContext$1`) inherits `Object.equals` on its
/// singleton, while frame contexts count `equals`.
pub(crate) const TOOLTIP_CONTEXT_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| {
        if handle.key() != "empty" {
            return Ok(None);
        }
        Ok(Some(matches!(
            other,
            Value::Host(other)
                if other.kind() == HostKind::TooltipContext && other.key() == "empty"
        )))
    },
    instance_of: instance_of_class,
    crosses_jars: |_, _| false,
    compare_to: None,
};

/// `TooltipDisplay` handle methods: `Object` methods are counted.
pub(crate) const TOOLTIP_DISPLAY_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |_, _, _| Ok(None),
    instance_of: tooltip_display_instance_of,
    crosses_jars: |_, _| false,
    compare_to: None,
};

/// Opaque `TooltipFlag` handle methods: `Object` methods are counted.
pub(crate) const TOOLTIP_FLAG_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |_, _, _| Ok(None),
    instance_of: instance_of_class,
    crosses_jars: |_, _| false,
    compare_to: None,
};

/// Concrete `net.minecraft.world.item.TooltipFlag$Default` record methods
/// matching Java 25 `ObjectMethods.bootstrap` (`Default[advanced=.., creative=..]`).
pub(crate) const TOOLTIP_FLAG_DEFAULT_METHODS: Methods = Methods {
    to_string: |handle, _| {
        Ok(
            parse_tooltip_flag_default(handle.key()).map(|(advanced, creative)| {
                format!("Default[advanced={advanced}, creative={creative}]")
            }),
        )
    },
    hash_code: |handle, _| {
        Ok(
            parse_tooltip_flag_default(handle.key()).map(|(advanced, creative)| {
                let first = if advanced { 1231_i32 } else { 1237_i32 };
                let second = if creative { 1231_i32 } else { 1237_i32 };
                first.wrapping_mul(31).wrapping_add(second)
            }),
        )
    },
    equals: |handle, other, _| {
        let Some(mine) = parse_tooltip_flag_default(handle.key()) else {
            return Ok(None);
        };
        match other {
            Value::Host(other) if other.kind() == HostKind::TooltipFlagDefault => {
                Ok(parse_tooltip_flag_default(other.key()).map(|theirs| mine == theirs))
            }
            // An opaque `TooltipFlag` or unmodelled `new TooltipFlag$Default`
            // might represent a `Default` with unknown fields: count it.
            Value::Host(other) if other.kind() == HostKind::TooltipFlag => Ok(None),
            other if crate::jar_object::object_class(other) == Some(TOOLTIP_FLAG_DEFAULT_CLASS) => {
                Ok(None)
            }
            _ => Ok(Some(false)),
        }
    },
    instance_of: |_, wanted, hierarchy| {
        tooltip_flag_default_class_instance_of(TOOLTIP_FLAG_DEFAULT_CLASS, wanted, hierarchy)
    },
    crosses_jars: |_, _| true,
    compare_to: None,
};

/// A Fabric `ItemTooltipCallback`'s arguments: the stack, its context, the
/// flag and the lines.
pub(crate) fn tooltip_arguments() -> Vec<Value> {
    vec![
        tooltip_stack(),
        tooltip_context(),
        tooltip_flag_from_frame(),
        marker(TOOLTIP_LIST),
    ]
}

/// A `java.awt.Color` constant: `client/awt-color/<aarrggbb>`.
use crate::client_values::AWT_COLOR;
/// `java.awt.Color`'s named constants (both spellings resolve).
const AWT_COLORS: &[(&str, u32)] = &[
    ("WHITE", 0xffff_ffff),
    ("LIGHT_GRAY", 0xffc0_c0c0),
    ("GRAY", 0xff80_8080),
    ("DARK_GRAY", 0xff40_4040),
    ("BLACK", 0xff00_0000),
    ("RED", 0xffff_0000),
    ("PINK", 0xffff_afaf),
    ("ORANGE", 0xffff_c800),
    ("YELLOW", 0xffff_ff00),
    ("GREEN", 0xff00_ff00),
    ("MAGENTA", 0xffff_00ff),
    ("CYAN", 0xff00_ffff),
    ("BLUE", 0xff00_00ff),
];

pub(crate) fn awt_constant(name: &str) -> Option<Value> {
    let canonical = match name {
        "lightGray" => "LIGHT_GRAY",
        "darkGray" => "DARK_GRAY",
        "white" => "WHITE",
        "gray" => "GRAY",
        "black" => "BLACK",
        "red" => "RED",
        "pink" => "PINK",
        "orange" => "ORANGE",
        "yellow" => "YELLOW",
        "green" => "GREEN",
        "magenta" => "MAGENTA",
        "cyan" => "CYAN",
        "blue" => "BLUE",
        other => other,
    };
    AWT_COLORS
        .iter()
        .find(|(field, _)| *field == canonical)
        .map(|(_, argb)| marker(format!("{AWT_COLOR}{argb:08x}")))
}

/// `Runnable`s one firing may queue with `Minecraft.execute`.
const MAX_TASKS: usize = 64;
/// A vanilla mapping read through `options.keyJump`...: `client/vanilla-key/key.jump`.
const VANILLA_KEY: &str = "client/vanilla-key/";
use crate::client_values::CATEGORY;
/// The categories `KeyMapping.Category` registers itself (`minecraft:<name>`).
const VANILLA_CATEGORIES: &[&str] = &[
    "movement",
    "misc",
    "multiplayer",
    "gameplay",
    "inventory",
    "creative",
    "spectator",
    "debug",
];

const KEY_MAPPING: &str = "net/minecraft/client/KeyMapping";
const KEY_CATEGORY: &str = "net/minecraft/client/KeyMapping$Category";
const INPUT_TYPE: &str = "com/mojang/blaze3d/platform/InputConstants$Type";
const INPUT_CONSTANTS: &str = "com/mojang/blaze3d/platform/InputConstants";
const INPUT_KEY_PREFIX: &str = "client/input-key/";
const MINECRAFT_CLASS: &str = "net/minecraft/client/Minecraft";
const OPTIONS_CLASS: &str = "net/minecraft/client/Options";
const FONT_CLASS: &str = "net/minecraft/client/gui/Font";
/// The leading arguments of the `GuiGraphicsExtractor` overloads that draw a
/// `FormattedCharSequence`.
const FONT_CHAR_SEQUENCE_ARGS: &str =
    "(Lnet/minecraft/client/gui/Font;Lnet/minecraft/util/FormattedCharSequence;";
const WORD_WRAP: &str =
    "(Lnet/minecraft/client/gui/Font;Lnet/minecraft/network/chat/FormattedText;IIII)I";
const WORD_WRAP_SHADOW: &str =
    "(Lnet/minecraft/client/gui/Font;Lnet/minecraft/network/chat/FormattedText;IIIIZ)I";
/// 26.3 `blitSprite(RenderPipeline, Identifier, x, y, w, h…)`'s leading arguments.
const BLIT_SPRITE_ARGS: &str =
    "(Lcom/mojang/renderpearl/api/pipeline/RenderPipeline;Lnet/minecraft/resources/Identifier;";

/// How a 26.3 whole-sprite `blitSprite` overload tints.
#[derive(Clone, Copy)]
enum SpriteTint {
    /// `(…IIII)V`: `-1`.
    White,
    /// `(…IIIII)V`: the colour argument.
    Colour,
    /// `(…IIIIF)V`: `ARGB.white(alpha)`.
    Alpha,
}

fn blit_sprite_overload(descriptor: &str) -> Option<SpriteTint> {
    Some(match descriptor.strip_prefix(BLIT_SPRITE_ARGS)? {
        "IIII)V" => SpriteTint::White,
        "IIIII)V" => SpriteTint::Colour,
        "IIIIF)V" => SpriteTint::Alpha,
        _ => return None,
    })
}

/// 26.3 `ARGB.white(float alpha)`: `as8BitChannel(alpha) << 24 | 0xFFFFFF`, where
/// `as8BitChannel` is `Mth.floor(alpha * 255.0f)` and `Mth.floor(float)` is
/// `(int) Math.floor((double) f)` (offsets 0-6): the float product widened, floored as a double,
/// then Java's saturating `d2i` (Rust's `as`; NaN is 0). An alpha outside `[0, 1]` shifts its
/// channel's low byte into the alpha as Java's `ishl` does: `-1e10f`, `-inf` and `-9e6f` floor to
/// `Integer.MIN_VALUE`, whose shifted byte is 0, so their colour is `0x00FFFFFF`.
fn argb_white(alpha: f32) -> u32 {
    let channel = f64::from(alpha * 255.0).floor() as i32;
    (channel.wrapping_shl(24) | 0x00ff_ffff) as u32
}
const RENDER_PIPELINES: &str = "net/minecraft/client/renderer/RenderPipelines";
const RENDER_PIPELINE_DESC: &str = "Lcom/mojang/renderpearl/api/pipeline/RenderPipeline;";
pub(crate) const GUI_TEXTURED_PIPELINE: &str = "client/render-pipeline/gui_textured";
const KEY_HELPER: &str = "net/fabricmc/fabric/api/client/keymapping/v1/KeyMappingHelper";
const HUD_REGISTRY: &str = "net/fabricmc/fabric/api/client/rendering/v1/hud/HudElementRegistry";
/// `HudElement.extractRenderState` and NeoForge's `GuiLayer.render` share it.
const LAYER_DESCRIPTOR: &str =
    "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/client/DeltaTracker;)V";

/// Key mappings one jar may register; more are refused (a jar with hundreds
/// is generating them, and each becomes a Controls row).
pub const MAX_KEYS_PER_JAR: usize = 64;
/// HUD elements one jar may register.
pub const MAX_LAYERS_PER_JAR: usize = 32;
/// Draw operations one callback may record in one frame; one more fails it.
pub const MAX_DRAWS_PER_CALL: usize = 2048;
/// Characters of one drawn string or chat line; longer ones are cut.
const MAX_TEXT: usize = 512;
/// `pushMatrix` depth.
const MAX_POSE_DEPTH: usize = 16;
/// Chat lines one firing may add.
const MAX_MESSAGES: usize = 16;

/// `Options` key fields (`options.keyJump`) and the vanilla mapping each is.
const OPTION_KEYS: &[(&str, &str)] = &[
    ("keyUp", "key.forward"),
    ("keyLeft", "key.left"),
    ("keyDown", "key.back"),
    ("keyRight", "key.right"),
    ("keyJump", "key.jump"),
    ("keyShift", "key.sneak"),
    ("keySprint", "key.sprint"),
    ("keyInventory", "key.inventory"),
    ("keySwapOffhand", "key.swapOffhand"),
    ("keyDrop", "key.drop"),
    ("keyUse", "key.use"),
    ("keyAttack", "key.attack"),
    ("keyPickItem", "key.pickItem"),
    ("keyChat", "key.chat"),
    ("keyPlayerList", "key.playerlist"),
    ("keyCommand", "key.command"),
    ("keyScreenshot", "key.screenshot"),
    ("keyTogglePerspective", "key.togglePerspective"),
    ("keySmoothCamera", "key.smoothCamera"),
    ("keyFullscreen", "key.fullscreen"),
    ("keyAdvancements", "key.advancements"),
];

/// One key mapping a jar registered.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientKeyMapping {
    /// The `KeyMapping` object marker the mod keeps and later asks
    /// `consumeClick` of.
    pub marker: String,
    /// Its name, a translation key of the mod's own lang file
    /// (`key.examplemod.zoom`); also its `options.txt` name (`key_<name>`).
    pub name: String,
    /// Its default key as `InputConstants` names it (`key.keyboard.c`,
    /// `key.mouse.middle`), `key.keyboard.unknown` when the code has no name.
    pub default_key: String,
    /// Its category: an identifier (`minecraft:misc`, `examplemod:main`), or
    /// a pre-26.3 mod's translation key (`key.categories.examplemod`).
    pub category: String,
    /// The API it came through, for diagnostics.
    pub source: &'static str,
}

/// One HUD element a jar registered.
#[derive(Clone, Debug, PartialEq)]
pub struct HudLayer {
    /// Its identifier (`examplemod:coordinates`).
    pub id: String,
    /// The API it came through, for diagnostics.
    pub source: &'static str,
    pub callback: ListenerCallback,
}

/// The local player as the façade shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientPlayer {
    pub uuid: u128,
    pub name: String,
    pub entity_id: i32,
    pub pos: [f64; 3],
    /// Degrees, `getYRot`/`getXRot`.
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub max_health: f32,
    pub creative: bool,
    pub spectator: bool,
    pub sneaking: bool,
    /// `LocalPlayer.isUsingItem`, filled by the game for a frame.
    pub using_item: Option<bool>,
    /// `LocalPlayer.isSleeping`, filled by the game for a frame.
    pub sleeping: Option<bool>,
    /// `Entity.isSilent`, filled by the game for a frame (`None` refuses).
    pub silent: Option<bool>,
    pub sprinting: bool,
    pub on_ground: bool,
    /// `isFallFlying`: gliding with elytra.
    pub fall_flying: bool,
    /// Its `GameType` constant: `SURVIVAL`, `CREATIVE`, `ADVENTURE`, `SPECTATOR`.
    pub game_mode: String,
    /// The main inventory, 36 stacks (0..8 the hotbar); an empty slot is
    /// [`ClientStack::empty`].
    pub inventory: Vec<ClientStack>,
    /// The selected hotbar slot, 0..8.
    pub selected: usize,
    /// Armour: feet, legs, chest, head (`Inventory` slots 36..39).
    pub armor: [ClientStack; 4],
    pub offhand: ClientStack,
    pub effects: Vec<ClientEffect>,
    pub food: i32,
    pub saturation: f32,
    /// `getArmorValue`.
    pub armor_value: i32,
    pub absorption: f32,
    pub air: i32,
    pub max_air: i32,
    pub xp_level: i32,
    /// Progress to the next level, 0..1.
    pub xp_progress: f32,
    pub xp_total: i32,
}

/// The camera as the façade shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientCamera {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub detached: bool,
}

/// The client level as the façade shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientLevel {
    /// Dimension id (`minecraft:overworld`).
    pub dimension: String,
    pub game_time: i64,
    pub day_time: i64,
    /// `getMinY` and `getHeight`: the dimension's build limits.
    pub min_y: i32,
    pub height: i32,
}

/// What `Font.width` and `Font.split` need: the game font's advances and the translations a
/// translatable component resolves to, and the game's own answers for exact texts
/// (`client_font.rs`). Built by the game and shared. Every answer is exact or counted: a glyph past
/// the table, a translation the table lacks, a style the model doesn't read, each counts and is
/// asked ([`ClientFrameOutput::font_questions`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextMetrics {
    /// Advance of each char below `advances.len()`, in GUI pixels (`GlyphInfo.getAdvance(false)`
    /// in the default font).
    pub advances: Vec<f32>,
    /// Translation key → text, for translatable components.
    pub translations: BTreeMap<String, String>,
    /// The game's `Font.width` / `Font.split` of exact texts, which an embedder fills from its
    /// own font after a frame asked for them ([`ClientFrameOutput::font_questions`]).
    pub answers: FontAnswers,
}

/// The client state one firing sees, snapshotted by the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientSnapshot {
    /// `Minecraft.player`; `None` outside a world.
    pub player: Option<ClientPlayer>,
    /// `Minecraft.level`; `None` outside a world.
    pub level: Option<ClientLevel>,
    pub paused: bool,
    /// The world is this client's own (single player).
    pub single_player: bool,
    /// `options.hideGui` (F1).
    pub hide_gui: bool,
    /// `Hud.getGuiTicks`: the HUD's tick counter.
    pub gui_ticks: i32,
    /// `options.renderDistance()`, in chunks.
    pub render_distance: i32,
    /// `options.mainHand()`: the player's main arm is the left one.
    pub left_handed: bool,
    /// The single-player world is open to LAN (`IntegratedServer.isPublished`).
    pub lan_open: bool,
    /// The camera: eye position, rotation (degrees) and whether it is
    /// detached (third person).
    pub camera: Option<ClientCamera>,
    /// A screen is open (`Minecraft.screen` / `gui.screen()` is not null):
    /// the title, a menu, chat, an inventory. The façade has no screen
    /// object, so a callback that reads it then refuses.
    pub screen_open: bool,
    /// The debug screen (F3) shows.
    pub debug_screen: bool,
    /// `Window.getGuiScaledWidth/Height`, `GuiGraphicsExtractor.guiWidth/Height`.
    pub gui_size: [i32; 2],
    /// `Window.getWidth/Height`, in pixels.
    pub window_size: [i32; 2],
    /// `Window.getGuiScale`, filled by the game for a frame; absent stays counted.
    pub gui_scale: Option<i32>,
    /// `DeltaTracker.getGameTimeDeltaPartialTick`.
    pub partial_tick: f32,
    pub fps: i32,
    /// The vanilla mappings held right now (`key.jump`...).
    pub vanilla_keys_down: Vec<String>,
    pub metrics: Arc<TextMetrics>,
    /// Item facts by id, for stacks a callback builds itself and for
    /// `Item` members; shared, built once by the game.
    pub items: Arc<ItemTable>,
    /// The stack a tooltip callback describes (`client/stack/tooltip`).
    pub tooltip_stack: Option<ClientStack>,
    /// The tooltip's lines so far, as plain text, which a tooltip listener's
    /// list starts with.
    pub tooltip_lines: Vec<String>,
    /// `TooltipFlag.isAdvanced` (F3+H).
    pub advanced_tooltips: bool,
    /// `TooltipFlag.isCreative` (creative inventory search tab).
    pub creative_tooltips: bool,
    /// `level.tickRateManager().tickrate()` for `TooltipContext.tickRate()`;
    /// absent stays counted (never defaults to `20.0f`).
    pub tick_rate: Option<f32>,
    /// Option values filled by the game snapshot; absent values stay counted.
    pub options: crate::client_frame::ClientOptions,
    /// The crosshair target (`Minecraft.hitResult`) filled by the game.
    pub hit_result: Option<crate::client_frame::ClientHitResult>,
    /// A container screen's frame (`AbstractContainerScreen` on a Sunrise mod menu screen,
    /// `screen_models.rs`). The embedder fills only `gui_size` and the presence of `player`
    /// and `level`; every other field keeps its default and is no fact. So, in this scope, an
    /// allow-list answers: the `GuiGraphicsExtractor` and its pose (minus anything measured with
    /// the font, whose metrics are not filled), and the `Minecraft` fields that are bare handles.
    /// Every other client-object member, the player's inventory and fields, `Minecraft.screen`
    /// and the options are refused and counted (`StdHost::screen_scope_call`).
    pub screen_scope: bool,
}

/// One registered mapping's input state, in [`StdHost::client_key_mappings`]
/// order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyPress {
    /// `isDown`.
    pub down: bool,
    /// Clicks `consumeClick` has not taken yet.
    pub clicks: u32,
}

/// Text a mod drew or sent.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClientText {
    /// A string (its `§` codes are formatting) or a literal component.
    Literal(String),
    /// A translatable component's key.
    Translatable(String),
    /// A styled component as vanilla's component JSON (`facade_text.rs`).
    Json(String),
}

/// One recorded `GuiGraphicsExtractor` call, in GUI pixels after the pose.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawOp {
    /// `text`/`centeredText`: `at` is where the text starts on screen and
    /// `scale` the pose's scale, which the game applies to the glyphs.
    Text {
        text: ClientText,
        at: [f32; 2],
        argb: u32,
        shadow: bool,
        scale: [f32; 2],
    },
    /// `fill`: `[x, y, w, h]`.
    Fill { rect: [f32; 4], argb: u32 },
    /// `fillGradient`: `top` to `bottom`, down the rectangle.
    Gradient {
        rect: [f32; 4],
        top: u32,
        bottom: u32,
    },
    /// `item`/`fakeItem` (`model`) or `itemDecorations` (`decorations`) of a
    /// stack, its top left at `at`, 16 GUI pixels square times `scale`.
    /// `slot` names where the stack came from so the game can draw the real
    /// stack (its components, glint and trims); the façade's copy is the
    /// fallback for a built one.
    Item {
        stack: ClientStack,
        slot: Option<StackSlot>,
        at: [f32; 2],
        scale: [f32; 2],
        model: bool,
        /// `Some(count text)` draws the decorations; an inner `None` is the
        /// stack's own count (`itemDecorations(font, stack, x, y)`).
        decorations: Option<Option<String>>,
    },
    /// `blitSprite` of a whole GUI sprite (`hud/heart/full`, a mod's `examplemod:gui/meter`)
    /// into `rect`, tinted `argb`: drawn as the sprite's `gui.scaling` metadata says (stretched,
    /// tiled or nine-sliced, `GuiGraphicsExtractor.blitSprite`'s switch), which the embedder's
    /// sprite atlas knows. A nine-slice's borders keep their size in GUI pixels, so the 26.3 arm
    /// records one only under a pose that does not scale.
    Sprite {
        sprite: String,
        rect: [f32; 4],
        argb: u32,
    },
    /// `item`/`fakeItem` (`model`) or `itemDecorations` (`decorations`, as on
    /// [`DrawOp::Item`]) of a runtime `ItemStack` rather than a frame stack: one jar code built,
    /// or a jar component's contents decoded. `stack` is what the embedder rebuilds the game's
    /// stack from (its item, count and component patch, with no opaque or unsupported entry);
    /// its top left at `at`, 16 GUI pixels square times `scale`. `seed` is the model draw's
    /// (`item(stack, x, y, seed)`; `0` for the overloads without one, and for decorations). Like
    /// [`DrawOp::Item`], the draw names no holder: `item` passes the local player and
    /// `fakeItem` none, which only an item model property that reads its holder tells apart.
    Stack {
        stack: crate::StackComponents,
        at: [f32; 2],
        scale: [f32; 2],
        model: bool,
        seed: i32,
        decorations: Option<Option<String>>,
    },
    /// A texture blit: `src` is `[u, v, region_w, region_h]` in texture pixels
    /// (preserving Java's `float` u/v), `dst` is `[x, y, w, h]` in GUI pixels
    /// after the pose (matching `Fill`, `Gradient`, and `Sprite`), `tex_size`
    /// is `[texture_width, texture_height]`, and `tint` is ARGB.
    Blit {
        texture: String,
        src: [f32; 4],
        dst: [f32; 4],
        tex_size: [i32; 2],
        tint: u32,
    },
}

/// A chat line or action-bar message a mod sent.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    /// `gui.getChat().addMessage`, `displayClientMessage(_, false)`,
    /// `sendSystemMessage`.
    Chat(ClientText),
    /// `displayClientMessage(_, true)`, `gui.setOverlayMessage`.
    Overlay(ClientText),
}

/// What one firing left for the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientFrameOutput {
    pub draws: Vec<DrawOp>,
    pub messages: Vec<ClientMessage>,
    /// The mappings' state after the callback (clicks it consumed are gone).
    pub keys: Vec<KeyPress>,
    /// `Minecraft.execute(runnable)`s queued, in order: the game runs them
    /// after the firing ([`NativeJar::run_client_task`]), as vanilla runs its
    /// queued tasks later in the same tick.
    pub tasks: Vec<Value>,
    /// A tooltip firing's lines after its listeners, when the frame had a
    /// tooltip; a line that is not text (null, another object) is dropped.
    pub tooltip: Option<Vec<ClientText>>,
    /// Client-side level/local-player sounds recorded during this frame, in call order.
    pub sounds: Vec<crate::level_sound::LevelSoundEffect>,
    /// The `Font.width` / `Font.split` questions the frame could not answer exactly (each call
    /// counted), in the order first asked: what an embedder answers ([`FontAnswers`]) before it
    /// runs the call again.
    pub font_questions: Vec<FontQuestion>,
    /// The counted calls those questions refused, by the key `StdHost::unknown_host_calls`
    /// counts them under (`net/minecraft/client/gui/Font#width…`, a `GuiGraphicsExtractor`
    /// draw that measured or wrapped). An embedder that runs a call again with its answers may
    /// drop exactly these from a discarded run, and nothing else that run counted
    /// ([`crate::StdHost::unknown_calls_since`]).
    pub font_refusals: BTreeMap<String, usize>,
}

/// How a `KeyMapping` was constructed.
#[derive(Clone, Debug, PartialEq)]
struct KeyInit {
    name: String,
    mouse: bool,
    code: i32,
    category: Option<String>,
    default_key: String,
    key: String,
    /// The actual category object passed to the constructor. A jar may pass
    /// its own `new Category(id)`, which is equal by the record's id but is
    /// not the registered singleton by reference identity.
    category_value: Option<Value>,
}

/// One affine pose without rotation: `[sx, sy, tx, ty]`.
type Pose = [f32; 4];
const IDENTITY: Pose = [1.0, 1.0, 0.0, 0.0];

/// The client part of a [`StdHost`]: registrations (kept for the host's
/// life) and the current firing (only while one runs).
#[derive(Clone, Debug, Default)]
pub(crate) struct ClientState {
    /// `KeyMapping` object marker → its constructor's arguments.
    key_inits: BTreeMap<String, KeyInit>,
    pub(crate) registrations: client_registrations::State,
    /// `KeyMapping.Category` object marker → its identifier.
    categories: BTreeMap<String, String>,
    /// The ids `KeyMapping.Category.register` made a category of.
    registered_categories: std::collections::BTreeSet<String>,
    keys: Vec<ClientKeyMapping>,
    /// Registrations that could not become a mapping or layer, with why.
    refused: Vec<String>,
    /// HUD registrations as recorded: (id, element value, source).
    layers: Vec<(String, Value, &'static str)>,
    pub(crate) frame: Option<Arc<ClientSnapshot>>,
    /// Identity `(uuid, entity_id)` of the `LocalPlayer` in the previous client
    /// frame, used to forget stale `client/player` Mixin fields on disconnect or
    /// rejoin even when `begin_client_frame` is driven directly outside `mod/`.
    last_player: Option<(u128, i32)>,
    presses: Vec<KeyPress>,
    draws: Vec<DrawOp>,
    messages: Vec<ClientMessage>,
    poses: Vec<Pose>,
    /// Stacks the firing's callbacks built (`new ItemStack`).
    made: items::MadeStacks,
    /// A built stack's object marker → its slot.
    made_aliases: BTreeMap<String, StackSlot>,
    tasks: Vec<Value>,
    pub(crate) sounds: Vec<crate::level_sound::LevelSoundEffect>,
    /// Currently open client screen (reused across `open_client_screen` ->
    /// `draw_client_screen_bg`, and closed via virtual `removed()` before opening
    /// a different screen so slot listeners never leak).
    active_screen: Option<ActiveScreen>,
    /// The data values the bridge last wrote into the active client menu (`setData`), by
    /// data-slot index.
    pub(crate) screen_menu_data: Vec<Option<i32>>,
    /// `Screen.extractBackground` (the in-game backdrop Sunrise draws itself) ran in this
    /// frame's `extractBackground` chain.
    pub(crate) screen_backdrop: bool,
    /// The frame's unanswered font questions (`client_font.rs`).
    font_questions: Vec<FontQuestion>,
    /// A font question was just asked, so the refusal the surface makes next is the font's:
    /// set by `ask_font`, taken by `refuse_client_call`, and cleared as each client call starts,
    /// so it never outlives the call that asked.
    font_refusal_pending: bool,
    /// The counted keys of the font's refusals this frame, with how many.
    font_refusals: BTreeMap<String, usize>,
    /// `Font.split` line handles (`client/char-sequence/<n>`) → their text, for this frame: a
    /// line kept into a later frame is no longer read (its use counts).
    char_sequences: BTreeMap<String, ClientText>,
    /// The next line handle's number; never reused.
    next_char_sequence: u64,
}

#[derive(Clone, Debug)]
struct ActiveScreen {
    menu_type: Value,
    menu: Value,
    title: Value,
    gui_size: [i32; 2],
    screen: Value,
}

/// A client object or value: a game handle (#156 P16d) for a `client/…` or
/// `key-category/…` marker, the text itself for any other (an enum constant,
/// a callback, a `String` answer).
fn marker(text: impl Into<String>) -> Value {
    crate::game_object::client_value(text.into())
}

/// A `String` answer: the text itself, never a client object it might spell.
fn text(text: impl Into<String>) -> Value {
    Value::Ref(Some(text.into()))
}

fn flag(value: bool) -> Value {
    Value::Int(i32::from(value))
}

fn int_arg(args: &[Value], at: usize) -> Option<i32> {
    match args.get(at)? {
        Value::Int(value) => Some(*value),
        _ => None,
    }
}

fn float_arg(args: &[Value], at: usize) -> Option<f32> {
    match args.get(at)? {
        Value::Float(value) => Some(*value as f32),
        Value::Int(value) => Some(*value as f32),
        _ => None,
    }
}

fn cut(text: &str) -> String {
    text.chars().take(MAX_TEXT).collect()
}

/// A drawn or sent text argument: a `String` or a component as it is when
/// drawn or sent (its state in `states`; a `FormattedCharSequence` is the
/// component it came from). `None` for null.
fn text_arg(value: &Value, states: &crate::host_component::States) -> Option<ClientText> {
    let json = crate::facade_text::json_now(value, Some(states));
    if let Some(literal) = json.as_ref().and_then(crate::host_component::bare_literal) {
        return Some(ClientText::Literal(cut(&literal)));
    }
    if let Some(key) = json.as_ref().and_then(crate::host_component::bare_key) {
        return Some(ClientText::Translatable(cut(&key)));
    }
    if let Some(json) = json {
        // Bounded by the component model itself (64 KiB).
        return Some(ClientText::Json(json.to_string()));
    }
    let text = expect_ref(value)?;
    Some(ClientText::Literal(cut(text)))
}

/// The `InputConstants` name of a key code: an SDL scancode for a keyboard
/// key (`InputConstants.KEY_C` is 6 in 26.3), a button for a mouse one.
pub fn input_key_name(mouse: bool, code: i32) -> String {
    if mouse {
        return match code {
            1 => "key.mouse.left".into(),
            2 => "key.mouse.middle".into(),
            3 => "key.mouse.right".into(),
            n => format!("key.mouse.{n}"),
        };
    }
    if code == 0 {
        return "key.keyboard.unknown".into();
    }
    KEYBOARD_NAMES
        .iter()
        .find(|(scancode, _)| *scancode == code)
        .map_or_else(
            || format!("key.keyboard.{code}"),
            |(_, name)| (*name).into(),
        )
}

fn input_key_marker(mouse: bool, code: i32) -> String {
    format!(
        "{INPUT_KEY_PREFIX}{}{code}",
        if mouse { "MOUSE/" } else { "KEYBOARD/" }
    )
}

fn input_key_parts(marker: &str) -> Option<(bool, i32)> {
    let rest = marker.strip_prefix(INPUT_KEY_PREFIX)?;
    let (kind, value) = rest.split_once('/')?;
    Some((kind == "MOUSE", value.parse().ok()?))
}

fn input_key_from_name(name: &str) -> Result<String, String> {
    if name == "key.keyboard.unknown" {
        return Ok(input_key_marker(false, 0));
    }
    for (known, code) in [
        ("key.mouse.left", 1),
        ("key.mouse.middle", 2),
        ("key.mouse.right", 3),
    ] {
        if name == known {
            return Ok(input_key_marker(true, code));
        }
    }
    for (code, known) in KEYBOARD_NAMES {
        if *known == name {
            return Ok(input_key_marker(false, *code));
        }
    }
    for (prefix, mouse) in [("key.keyboard.", false), ("key.mouse.", true)] {
        if let Some(number) = name.strip_prefix(prefix) {
            let code = number
                .parse::<i32>()
                .map_err(|_| format!("For input string: \"{number}\""))?;
            return Ok(input_key_marker(mouse, code));
        }
    }
    Err(format!("Unknown key name: {name}"))
}

/// `InputConstants` keyboard names by SDL scancode (26.3): the subset a key
/// mapping's default uses. The full table is `horizon_client_input::key_names`,
/// which this crate cannot depend on; the game re-checks every default there.
const KEYBOARD_NAMES: &[(i32, &str)] = &[
    (4, "key.keyboard.a"),
    (5, "key.keyboard.b"),
    (6, "key.keyboard.c"),
    (7, "key.keyboard.d"),
    (8, "key.keyboard.e"),
    (9, "key.keyboard.f"),
    (10, "key.keyboard.g"),
    (11, "key.keyboard.h"),
    (12, "key.keyboard.i"),
    (13, "key.keyboard.j"),
    (14, "key.keyboard.k"),
    (15, "key.keyboard.l"),
    (16, "key.keyboard.m"),
    (17, "key.keyboard.n"),
    (18, "key.keyboard.o"),
    (19, "key.keyboard.p"),
    (20, "key.keyboard.q"),
    (21, "key.keyboard.r"),
    (22, "key.keyboard.s"),
    (23, "key.keyboard.t"),
    (24, "key.keyboard.u"),
    (25, "key.keyboard.v"),
    (26, "key.keyboard.w"),
    (27, "key.keyboard.x"),
    (28, "key.keyboard.y"),
    (29, "key.keyboard.z"),
    (30, "key.keyboard.1"),
    (31, "key.keyboard.2"),
    (32, "key.keyboard.3"),
    (33, "key.keyboard.4"),
    (34, "key.keyboard.5"),
    (35, "key.keyboard.6"),
    (36, "key.keyboard.7"),
    (37, "key.keyboard.8"),
    (38, "key.keyboard.9"),
    (39, "key.keyboard.0"),
    (40, "key.keyboard.enter"),
    (41, "key.keyboard.escape"),
    (42, "key.keyboard.backspace"),
    (43, "key.keyboard.tab"),
    (44, "key.keyboard.space"),
    (45, "key.keyboard.minus"),
    (46, "key.keyboard.equal"),
    (47, "key.keyboard.left.bracket"),
    (48, "key.keyboard.right.bracket"),
    (49, "key.keyboard.backslash"),
    (51, "key.keyboard.semicolon"),
    (52, "key.keyboard.apostrophe"),
    (53, "key.keyboard.grave.accent"),
    (54, "key.keyboard.comma"),
    (55, "key.keyboard.period"),
    (56, "key.keyboard.slash"),
    (57, "key.keyboard.caps.lock"),
    (58, "key.keyboard.f1"),
    (59, "key.keyboard.f2"),
    (60, "key.keyboard.f3"),
    (61, "key.keyboard.f4"),
    (62, "key.keyboard.f5"),
    (63, "key.keyboard.f6"),
    (64, "key.keyboard.f7"),
    (65, "key.keyboard.f8"),
    (66, "key.keyboard.f9"),
    (67, "key.keyboard.f10"),
    (68, "key.keyboard.f11"),
    (69, "key.keyboard.f12"),
    (70, "key.keyboard.print.screen"),
    (71, "key.keyboard.scroll.lock"),
    (72, "key.keyboard.pause"),
    (73, "key.keyboard.insert"),
    (74, "key.keyboard.home"),
    (75, "key.keyboard.page.up"),
    (76, "key.keyboard.delete"),
    (77, "key.keyboard.end"),
    (78, "key.keyboard.page.down"),
    (79, "key.keyboard.right"),
    (80, "key.keyboard.left"),
    (81, "key.keyboard.down"),
    (82, "key.keyboard.up"),
    (83, "key.keyboard.num.lock"),
    (84, "key.keyboard.keypad.divide"),
    (85, "key.keyboard.keypad.multiply"),
    (86, "key.keyboard.keypad.subtract"),
    (87, "key.keyboard.keypad.add"),
    (88, "key.keyboard.keypad.enter"),
    (89, "key.keyboard.keypad.1"),
    (90, "key.keyboard.keypad.2"),
    (91, "key.keyboard.keypad.3"),
    (92, "key.keyboard.keypad.4"),
    (93, "key.keyboard.keypad.5"),
    (94, "key.keyboard.keypad.6"),
    (95, "key.keyboard.keypad.7"),
    (96, "key.keyboard.keypad.8"),
    (97, "key.keyboard.keypad.9"),
    (98, "key.keyboard.keypad.0"),
    (99, "key.keyboard.keypad.decimal"),
    (224, "key.keyboard.left.control"),
    (225, "key.keyboard.left.shift"),
    (226, "key.keyboard.left.alt"),
    (227, "key.keyboard.left.win"),
    (228, "key.keyboard.right.control"),
    (229, "key.keyboard.right.shift"),
    (230, "key.keyboard.right.alt"),
    (231, "key.keyboard.right.win"),
];

/// A category marker's identifier: `key-category/<namespace>:<path>`.
fn category_id(value: &Value, categories: &BTreeMap<String, String>) -> Option<String> {
    let text = crate::game_object::text(value)?;
    if let Some(id) = text.strip_prefix(CATEGORY) {
        return Some(id.to_owned());
    }
    // A pre-26.3 mod's `String` category (`key.categories.examplemod`).
    if text.starts_with("key.") {
        return Some(text.clone());
    }
    categories.get(text).cloned()
}

/// `KeyMapping.<init>`'s arguments, for every 26.3 overload (and the
/// `String`-category ones of mods built before 1.21.9).
fn key_init(
    descriptor: &str,
    args: &[Value],
    categories: &BTreeMap<String, String>,
) -> Option<KeyInit> {
    let name = expect_ref(args.first()?)?.clone();
    let typed = descriptor
        .starts_with("(Ljava/lang/String;Lcom/mojang/blaze3d/platform/InputConstants$Type;I");
    let (mouse, code, category) = if typed {
        let mouse = expect_ref(args.get(1)?).is_some_and(|t| t.ends_with("/MOUSE"));
        (mouse, int_arg(args, 2)?, args.get(3)?)
    } else if descriptor.starts_with("(Ljava/lang/String;I") {
        (false, int_arg(args, 1)?, args.get(2)?)
    } else {
        return None;
    };
    let key = input_key_marker(mouse, code);
    Some(KeyInit {
        name,
        mouse,
        code,
        category: category_id(category, categories),
        category_value: Some(category.clone()).filter(|value| !matches!(value, Value::Ref(None))),
        default_key: key.clone(),
        key,
    })
}

impl ClientState {
    fn in_frame(&self) -> bool {
        self.frame.is_some()
    }

    fn pose(&self) -> Pose {
        self.poses.last().copied().unwrap_or(IDENTITY)
    }

    fn register_key(&mut self, value: &Value, source: &'static str) {
        let Some(marker) = expect_ref(value) else {
            self.refused.push(format!("{source}: null key mapping"));
            return;
        };
        if self.keys.iter().any(|key| key.marker == *marker) {
            return;
        }
        let Some(init) = self.key_inits.get(marker) else {
            self.refused.push(format!(
                "{source}: {marker} was not built by a KeyMapping constructor"
            ));
            return;
        };
        if self.keys.len() >= MAX_KEYS_PER_JAR {
            self.refused.push(format!(
                "{source}: more than {MAX_KEYS_PER_JAR} key mappings"
            ));
            return;
        }
        self.keys.push(ClientKeyMapping {
            marker: marker.clone(),
            name: init.name.clone(),
            default_key: input_key_name(init.mouse, init.code),
            category: init
                .category
                .clone()
                .unwrap_or_else(|| "minecraft:misc".into()),
            source,
        });
    }

    fn register_layer(&mut self, id: &Value, element: &Value, source: &'static str) {
        if self.layers.len() >= MAX_LAYERS_PER_JAR {
            self.refused.push(format!(
                "{source}: more than {MAX_LAYERS_PER_JAR} HUD elements"
            ));
            return;
        }
        let (namespace, path) = identifier_parts(id);
        self.layers
            .push((format!("{namespace}:{path}"), element.clone(), source));
    }

    fn draw(&mut self, op: DrawOp) -> Result<(), ()> {
        if self.draws.len() >= MAX_DRAWS_PER_CALL {
            return Err(());
        }
        self.draws.push(op);
        Ok(())
    }

    fn message(&mut self, message: ClientMessage) {
        if self.messages.len() < MAX_MESSAGES {
            self.messages.push(message);
        }
    }

    /// `fill(x1, y1, x2, y2, color)` after the pose, corners in any order.
    fn fill(&mut self, corners: [i32; 4], argb: u32) -> Result<(), ()> {
        if argb >> 24 == 0 {
            return Ok(());
        }
        let rect = self.posed_rect(corners);
        self.draw(DrawOp::Fill { rect, argb })
    }

    fn posed_rect(&self, [x1, y1, x2, y2]: [i32; 4]) -> [f32; 4] {
        let [sx, sy, tx, ty] = self.pose();
        let (ax, bx) = (x1 as f32 * sx + tx, x2 as f32 * sx + tx);
        let (ay, by) = (y1 as f32 * sy + ty, y2 as f32 * sy + ty);
        [ax.min(bx), ay.min(by), (ax - bx).abs(), (ay - by).abs()]
    }

    fn text(
        &mut self,
        text: ClientText,
        x: i32,
        y: i32,
        argb: u32,
        shadow: bool,
    ) -> Result<(), ()> {
        // GuiGraphicsExtractor.text: a colour with no alpha draws nothing.
        if argb >> 24 == 0 {
            return Ok(());
        }
        let [sx, sy, tx, ty] = self.pose();
        self.draw(DrawOp::Text {
            text,
            at: [x as f32 * sx + tx, y as f32 * sy + ty],
            argb,
            shadow,
            scale: [sx, sy],
        })
    }
}

impl StdHost {
    /// A server's host (`with_environment_type("SERVER")`) has no client:
    /// there, none of this surface answers, and the host behaves exactly as
    /// it did before the surface existed.
    fn client_environment(&self) -> bool {
        self.environment_type != "SERVER"
    }

    /// Registration-only client APIs, in the order the jar called them.
    pub fn client_registration_summary(&self) -> serde_json::Value {
        client_registrations::summary(&self.client.registrations)
    }

    /// Whether a `MenuScreens.register` this host's jar made covers `menu_type`
    /// (`MenuScreens.getConstructor` is not null).
    pub fn client_menu_screen_registered(&self, menu_type: &Value) -> bool {
        self.client_menu_constructor(menu_type).is_some()
    }

    /// The current frame is a container screen's ([`ClientSnapshot::screen_scope`]).
    pub(crate) fn client_screen_scope(&self) -> bool {
        self.client
            .frame
            .as_ref()
            .is_some_and(|frame| frame.screen_scope)
    }

    /// Whether `Screen.extractBackground`'s own body (the backdrop Sunrise draws itself) ran
    /// since the last [`StdHost::client_screen_backdrop_reset`].
    pub fn client_screen_backdrop_ran(&self) -> bool {
        self.client.screen_backdrop
    }

    pub(crate) fn client_screen_backdrop_reset(&mut self) {
        self.client.screen_backdrop = false;
    }

    /// The GUI size the active screen was laid out for (its last `init` or `resize`).
    pub fn client_active_gui_size(&self) -> Option<[i32; 2]> {
        self.client
            .active_screen
            .as_ref()
            .map(|active| active.gui_size)
    }

    pub(crate) fn client_menu_constructor(&self, menu: &Value) -> Option<Value> {
        client_registrations::menu_constructor(&self.client.registrations, menu)
    }

    pub(crate) fn client_screen_layout(
        &self,
        screen: &Value,
    ) -> Option<crate::menu_models::ScreenLayout> {
        crate::menu_models::screen_layout(&self.menu_states, screen)
    }

    pub(crate) fn client_draws(&self) -> &[DrawOp] {
        &self.client.draws
    }

    pub(crate) fn client_gui_size(&self) -> Option<[i32; 2]> {
        self.client.frame.as_ref().map(|frame| frame.gui_size)
    }

    pub(crate) fn client_has_player(&self) -> bool {
        self.client
            .frame
            .as_ref()
            .is_some_and(|frame| frame.player.is_some())
    }

    pub(crate) fn client_inventory_stack(&self, container_slot: i32) -> Option<ClientStack> {
        let frame = self.client.frame.as_ref()?;
        // A container screen's frame fills no inventory: the read is no fact (counted).
        if frame.screen_scope {
            return None;
        }
        let player = frame.player.as_ref()?;
        match container_slot {
            0..=35 => Some(
                player
                    .inventory
                    .get(container_slot as usize)
                    .cloned()
                    .unwrap_or_else(ClientStack::empty),
            ),
            36..=39 => Some(
                player
                    .armor
                    .get((container_slot - 36) as usize)
                    .cloned()
                    .unwrap_or_else(ClientStack::empty),
            ),
            40 => Some(player.offhand.clone()),
            _ => None,
        }
    }

    pub(crate) fn client_matching_active_screen(
        &self,
        menu_type: &Value,
        menu: &Value,
        title: &Value,
        gui_size: [i32; 2],
    ) -> Option<Value> {
        let active = self.client.active_screen.as_ref()?;
        (active.menu_type == *menu_type
            && active.menu == *menu
            && active.title == *title
            && active.gui_size == gui_size)
            .then(|| active.screen.clone())
    }

    /// The active screen: its menu type, menu, title, the GUI size it was laid out for, and
    /// the screen.
    pub(crate) fn client_active_screen(&self) -> Option<(Value, Value, Value, [i32; 2], Value)> {
        let active = self.client.active_screen.as_ref()?;
        Some((
            active.menu_type.clone(),
            active.menu.clone(),
            active.title.clone(),
            active.gui_size,
            active.screen.clone(),
        ))
    }

    /// Forget the active screen (`Gui.setScreen(null)` switching it out), returning it.
    pub(crate) fn take_client_active_screen(&mut self) -> Option<Value> {
        self.client.active_screen.take().map(|active| active.screen)
    }

    /// Forget the data values written into the active client menu (its screen is gone).
    pub(crate) fn clear_client_screen_menus(&mut self) {
        self.client.screen_menu_data.clear();
    }

    pub(crate) fn client_active_screen_value(&self) -> Option<Value> {
        self.client
            .active_screen
            .as_ref()
            .map(|active| active.screen.clone())
    }

    pub(crate) fn set_client_active_screen(
        &mut self,
        menu_type: Value,
        menu: Value,
        title: Value,
        gui_size: [i32; 2],
        screen: Value,
    ) {
        self.client.active_screen = Some(ActiveScreen {
            menu_type,
            menu,
            title,
            gui_size,
            screen,
        });
    }

    pub(crate) fn client_menu_value_for_type(&self, menu_type: &Value) -> Option<Value> {
        crate::menu_models::menu_for_type(&self.menu_states, menu_type)
    }

    pub fn client_inventory_value(&self) -> Value {
        crate::game_object::client_value("client/inventory".to_owned())
    }

    fn refuse_client_call(&mut self, r: &MethodRef) -> NativeError {
        let key = format!("{}#{}{}", r.owner, r.name, r.descriptor);
        if std::mem::take(&mut self.client.font_refusal_pending) {
            *self.client.font_refusals.entry(key.clone()).or_default() += 1;
        }
        *self.unknown_host_calls.entry(key).or_default() += 1;
        NativeError::UnknownHostCall(r.clone())
    }

    /// The key mappings this host's jar registered, in order.
    pub fn client_key_mappings(&self) -> &[ClientKeyMapping] {
        &self.client.keys
    }

    /// Registrations that did not become a key mapping or HUD element, with
    /// why (a mapping built some other way, a jar over a bound).
    pub fn client_refused_registrations(&self) -> &[String] {
        &self.client.refused
    }

    /// The HUD elements this host's jar registered, resolved against `jar`:
    /// a lambda or an object of a jar class implementing the element's
    /// method. An element that resolves to neither is listed in the second
    /// value with why.
    pub fn client_hud_layers(&self, jar: &NativeJar) -> (Vec<HudLayer>, Vec<String>) {
        let mut layers = Vec::new();
        let mut refused = Vec::new();
        for (id, element, source) in &self.client.layers {
            let callback = match crate::callback_marker::lambda_of(element) {
                Some(marker) => Some(ListenerCallback::Indy {
                    target: marker.target,
                    captures: marker.captures,
                }),
                None => events::jar_object(jar, element).and_then(|(class, value)| {
                    ["extractRenderState", "render"]
                        .iter()
                        .find_map(|name| events::resolve_method(jar, class, name, LAYER_DESCRIPTOR))
                        .map(|method| ListenerCallback::Object {
                            receiver: value.clone(),
                            method,
                        })
                }),
            };
            match callback {
                Some(callback) => layers.push(HudLayer {
                    id: id.clone(),
                    source,
                    callback,
                }),
                None => refused.push(format!("{source} {id}: the element is not a jar callback")),
            }
        }
        (layers, refused)
    }

    /// Start a client firing: the markers answer from `snapshot`, and the
    /// jar's mappings (in [`Self::client_key_mappings`] order) have `keys`.
    pub fn begin_client_frame(&mut self, snapshot: Arc<ClientSnapshot>, keys: &[KeyPress]) {
        let next_player = snapshot.player.as_ref().map(|p| (p.uuid, p.entity_id));
        let prev_had_level = self
            .client
            .frame
            .as_ref()
            .is_some_and(|frame| frame.level.is_some());
        let prev_player = self.client.last_player;
        if prev_had_level && snapshot.level.is_none() {
            self.forget_all_game_objects();
        } else if prev_player.is_some() && prev_player != next_player {
            self.forget_game_object(PLAYER);
        }
        let state = &mut self.client;
        state.last_player = next_player;
        state.frame = Some(snapshot);
        state.presses = keys.to_vec();
        state.presses.resize(state.keys.len(), KeyPress::default());
        state.draws.clear();
        state.messages.clear();
        state.poses.clear();
        state.made.clear();
        state.made_aliases.clear();
        state.tasks.clear();
        state.sounds.clear();
        state.font_questions.clear();
        state.font_refusal_pending = false;
        state.font_refusals.clear();
        state.char_sequences.clear();
        let lines = state.frame.as_ref().and_then(|f| {
            f.tooltip_stack.as_ref()?;
            Some(
                f.tooltip_lines
                    .iter()
                    .map(|line| component_value("literal", line))
                    .collect::<Vec<_>>(),
            )
        });
        match lines {
            Some(lines) => {
                self.mutable_collections.insert(TOOLTIP_LIST.into(), lines);
            }
            None => {
                self.mutable_collections.remove(TOOLTIP_LIST);
            }
        }
    }

    /// End the firing [`Self::begin_client_frame`] started: what the
    /// callbacks drew and sent, and the mappings' state after them.
    pub fn end_client_frame(&mut self) -> ClientFrameOutput {
        // A tooltip line that is a component no read resolves (past the
        // bound, a cycle) is left out of the tooltip and counted.
        let unread = self
            .mutable_collections
            .get(TOOLTIP_LIST)
            .map_or(0, |lines| {
                lines
                    .iter()
                    .filter(|line| {
                        crate::host_component::is_component(line)
                            && crate::facade_text::json_now(line, Some(&self.component_states))
                                .is_none()
                    })
                    .count()
            });
        for _ in 0..unread {
            let _ = self.unknown_host_call(&MethodRef {
                owner: "net/minecraft/network/chat/Component".into(),
                name: "getString".into(),
                descriptor: "()Ljava/lang/String;".into(),
            });
        }
        let state = &mut self.client;
        state.frame = None;
        state.char_sequences.clear();
        state.poses.clear();
        state.made.clear();
        state.made_aliases.clear();
        ClientFrameOutput {
            draws: std::mem::take(&mut state.draws),
            messages: std::mem::take(&mut state.messages),
            keys: std::mem::take(&mut state.presses),
            tasks: std::mem::take(&mut state.tasks),
            tooltip: self.mutable_collections.remove(TOOLTIP_LIST).map(|lines| {
                lines
                    .iter()
                    .filter_map(|line| text_arg(line, &self.component_states))
                    .take(MAX_TOOLTIP_LINES)
                    .collect()
            }),
            sounds: std::mem::take(&mut self.client.sounds),
            font_questions: std::mem::take(&mut self.client.font_questions),
            font_refusals: std::mem::take(&mut self.client.font_refusals),
        }
    }

    /// A static field the client surface answers: input types, key
    /// categories, and the fields of the `Minecraft` and `Options` stand-ins
    /// (an instance field of a host object reaches the host as a static read
    /// without its receiver).
    pub(crate) fn client_static(&mut self, r: &MethodRef) -> Option<Result<Value, NativeError>> {
        // Chat style/color values and common tooltip constants are shared by
        // server-side callbacks too; their handles do not depend on a frame.
        if let Some(value) = text::text_static(r) {
            return Some(Ok(value));
        }
        match (r.owner.as_str(), r.name.as_str()) {
            (TOOLTIP_FLAG_CLASS | TOOLTIP_FLAG_DEFAULT_CLASS, "NORMAL") => {
                return Some(Ok(tooltip_flag_default_const(false, false)));
            }
            (TOOLTIP_FLAG_CLASS | TOOLTIP_FLAG_DEFAULT_CLASS, "ADVANCED") => {
                return Some(Ok(tooltip_flag_default_const(true, false)));
            }
            (TOOLTIP_DISPLAY_CLASS, "DEFAULT") => {
                return Some(Ok(crate::recipe_models::canonical_default_tooltip_display()));
            }
            (TOOLTIP_CONTEXT_CLASS, "EMPTY") => return Some(Ok(tooltip_context_empty())),
            _ => {}
        }
        // `java.awt.Color` is a JDK singleton family even on a server host;
        // model both the upper- and lower-case field spellings everywhere.
        if !self.client_environment() && r.owner != "java/awt/Color" {
            return None;
        }
        if let Some(value) = client_registrations::static_field(&mut self.client.registrations, r) {
            return Some(Ok(value));
        }
        let state = &self.client;
        let frame = state.frame.as_deref();
        let refuse = || Some(Err(NativeError::UnknownHostCall(r.clone())));
        if r.owner == MINECRAFT_CLASS && r.name == "hitResult" {
            return match frame.and_then(|f| f.hit_result.as_ref()) {
                Some(hit) => Some(Ok(crate::client_frame::hit_value(hit))),
                None if frame.is_some() => refuse(),
                None => None,
            };
        }
        match (r.owner.as_str(), r.name.as_str()) {
            (INPUT_TYPE, name @ ("KEYBOARD" | "MOUSE")) => {
                Some(Ok(marker(format!("enum/{INPUT_TYPE}/{name}"))))
            }
            (INPUT_CONSTANTS, "UNKNOWN")
                if r.descriptor == "Lcom/mojang/blaze3d/platform/InputConstants$Key;" =>
            {
                Some(Ok(marker(input_key_marker(false, 0))))
            }
            (KEY_CATEGORY, name) if r.descriptor == format!("L{KEY_CATEGORY};") => Some(Ok(
                marker(format!("{CATEGORY}minecraft:{}", name.to_ascii_lowercase())),
            )),
            (MINECRAFT_CLASS, name) => Some(Ok(match name {
                "player" => frame
                    .and_then(|f| f.player.as_ref())
                    .map_or(Value::Ref(None), |_| marker(PLAYER)),
                "level" => frame
                    .and_then(|f| f.level.as_ref())
                    .map_or(Value::Ref(None), |_| marker(LEVEL)),
                // No screen: null. An open one has no stand-in, and refuses. In a container
                // screen's own frame the screen is open: refuse.
                "screen" if frame.is_some_and(|f| f.screen_open || f.screen_scope) => {
                    return refuse();
                }
                "screen" => Value::Ref(None),
                "options" => marker(OPTIONS),
                "gui" => marker(GUI),
                "font" => marker(FONT),
                "gameMode" => frame
                    .and_then(|f| f.player.as_ref())
                    .map_or(Value::Ref(None), |_| marker(GAME_MODE)),
                "gameRenderer" => marker(GAME_RENDERER),
                "debugEntries" => marker(DEBUG),
                // The launcher's configured java.io.File is outside this
                // bounded host. Count it instead of forging a String-shaped
                // File marker.
                "gameDirectory" => return refuse(),
                _ if frame.is_some() => return refuse(),
                _ => return None,
            })),
            (OPTIONS_CLASS, name) => {
                if let Some((_, mapping)) = OPTION_KEYS.iter().find(|(field, _)| *field == name) {
                    return Some(Ok(marker(format!("{VANILLA_KEY}{mapping}"))));
                }
                match (name, frame) {
                    ("hideGui", Some(frame)) if frame.screen_scope => refuse(),
                    ("hideGui", Some(frame)) => Some(Ok(flag(frame.hide_gui))),
                    ("keyDebugModifier", _) => {
                        Some(Ok(marker(format!("{VANILLA_KEY}key.debug.modifier"))))
                    }
                    (_, Some(_)) => refuse(),
                    (_, None) => None,
                }
            }
            (FONT_CLASS, "lineHeight") => Some(Ok(Value::Int(9))),
            // 26.3 RenderPipelines.<clinit> offsets 7793-7808: GUI_TEXTURED
            // built with location "pipeline/gui_textured".
            (RENDER_PIPELINES, "GUI_TEXTURED") if r.descriptor == RENDER_PIPELINE_DESC => {
                Some(Ok(marker(GUI_TEXTURED_PIPELINE)))
            }
            // `java.awt.Color`'s constants, which HUD mods turn into ARGB
            // ints with `getRGB()` (durabilityviewer's indicator colours).
            ("java/awt/Color", name) => awt_constant(name).map(Ok),
            // Item, block and effect constants answer only in a firing: an
            // entrypoint reading `Items.DIAMOND` keeps the answer it always got.
            ("net/minecraft/world/item/Items", name) if frame.is_some() => {
                items::constant_id(name).map(|id| Ok(marker(format!("{}{}", items::ITEM, id))))
            }
            // The id 26.3 registers for the constant (`vanilla_blocks.rs`); a
            // constant 26.3 lacks is no block (#239's review B2).
            ("net/minecraft/world/level/block/Blocks", name) if frame.is_some() => {
                crate::vanilla_blocks::field_id(name)
                    .map(|id| Ok(marker(format!("{}{id}", items::BLOCK))))
            }
            ("net/minecraft/world/effect/MobEffects", name) if frame.is_some() => {
                Some(Ok(marker(format!(
                    "{}{}",
                    items::EFFECT_HOLDER,
                    items::effect_constant_id(name)
                ))))
            }
            // `EquipmentSlot.VALUES`: every slot in declaration order.
            ("net/minecraft/world/entity/EquipmentSlot", "VALUES") => {
                let slots: Vec<Value> = [
                    "MAINHAND", "OFFHAND", "FEET", "LEGS", "CHEST", "HEAD", "BODY", "SADDLE",
                ]
                .iter()
                .map(|name| {
                    marker(format!(
                        "enum/net/minecraft/world/entity/EquipmentSlot/{name}"
                    ))
                })
                .collect();
                // `List.of(values())`: one `ListN` for the run.
                let made = self.shared_unmodifiable(
                    "EquipmentSlot.VALUES",
                    "java/util/ImmutableCollections$ListN",
                    slots,
                );
                if let Some(key) = crate::expect_ref(&made) {
                    self.jdk_list_of.insert(key.clone());
                }
                Some(Ok(made))
            }
            ("net/minecraft/world/item/ItemStack", "EMPTY") => {
                Some(Ok(marker(format!("{}empty", items::STACK))))
            }
            (
                "net/minecraft/world/entity/player/Player"
                | "net/minecraft/client/player/LocalPlayer"
                | "net/minecraft/client/player/AbstractClientPlayer",
                name,
            ) if frame.is_some() => {
                // A container screen's frame fills no player facts.
                if frame.is_some_and(|frame| frame.screen_scope) {
                    return refuse();
                }
                match frame.and_then(|frame| Self::client_player_field(frame, name)) {
                    Some(value) => Some(Ok(value)),
                    None => refuse(),
                }
            }
            ("net/minecraft/client/gui/Gui", "hud") => Some(Ok(marker(HUD))),
            _ => None,
        }
    }

    /// A static call the client surface answers: `Minecraft.getInstance`,
    /// key-mapping and HUD registration, category registration.
    pub(crate) fn client_invoke(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        // TextColor factories and `Item.TooltipContext.of(null)` are valid in
        // server callbacks as well.
        if let Some(value) = text::text_invoke(r, args) {
            return Some(Ok(value));
        }
        if r.owner == TOOLTIP_CONTEXT_CLASS
            && r.name == "of"
            && r.descriptor
                == "(Lnet/minecraft/world/level/Level;)Lnet/minecraft/world/item/Item$TooltipContext;"
            && args.first() == Some(&Value::Ref(None))
        {
            return Some(Ok(tooltip_context_empty()));
        }
        if !self.client_environment() {
            return None;
        }
        if let Some(value) = client_registrations::invoke(&mut self.client.registrations, r, args) {
            return Some(value);
        }
        let state = &mut self.client;
        match (r.owner.as_str(), r.name.as_str()) {
            (MINECRAFT_CLASS, "getInstance") => Some(Ok(marker(MINECRAFT))),
            (INPUT_CONSTANTS, "getKey")
                if r.descriptor
                    == "(Ljava/lang/String;)Lcom/mojang/blaze3d/platform/InputConstants$Key;" =>
            {
                let value = args.first()?;
                let name = match value {
                    // 26.3 dereferences the name before looking it up. Preserve
                    // that exact Java 25 NPE instead of counting a null input.
                    Value::Ref(None) => {
                        return Some(Err(NativeError::UncaughtException {
                            class: "java/lang/NullPointerException".into(),
                            message: Some(
                                r#"Cannot invoke "String.startsWith(String)" because "name" is null"#
                                    .into(),
                            ),
                            frame: None,
                        }));
                    }
                    _ => crate::game_object::text(value)?,
                };
                Some(match input_key_from_name(name) {
                    Ok(key) => Ok(marker(key)),
                    Err(message) if message.starts_with("Unknown key name:") => {
                        Err(NativeError::UncaughtException {
                            class: "java/lang/IllegalArgumentException".into(),
                            message: Some(message),
                            frame: None,
                        })
                    }
                    Err(message) => Err(NativeError::UncaughtException {
                        class: "java/lang/NumberFormatException".into(),
                        message: Some(message),
                        frame: None,
                    }),
                })
            }
            (KEY_HELPER, "registerKeyMapping") => {
                let mapping = args.first().cloned().unwrap_or(Value::Ref(None));
                state.register_key(&mapping, "KeyMappingHelper.registerKeyMapping");
                Some(Ok(mapping))
            }
            (HUD_REGISTRY, "addFirst" | "addLast") if args.len() == 2 => {
                state.register_layer(&args[0], &args[1], "HudElementRegistry.addLast");
                Some(Ok(Value::Void))
            }
            (HUD_REGISTRY, "attachElementBefore" | "attachElementAfter") if args.len() == 3 => {
                state.register_layer(&args[1], &args[2], "HudElementRegistry.attachElement");
                Some(Ok(Value::Void))
            }
            // One category per id: vanilla's own and a second registration of
            // one throw `IllegalArgumentException`, which is not modelled (counted).
            (KEY_CATEGORY, "register") => {
                let (namespace, path) = identifier_parts(args.first()?);
                let id = format!("{namespace}:{path}");
                let vanilla = namespace == "minecraft" && VANILLA_CATEGORIES.contains(&path);
                if vanilla || !state.registered_categories.insert(id.clone()) {
                    return Some(Err(NativeError::UnknownHostCall(r.clone())));
                }
                Some(Ok(marker(format!("{CATEGORY}{id}"))))
            }
            _ => None,
        }
    }

    /// An instance call the client surface answers, or `None` when the
    /// receiver is not one of its objects (the host's usual surface answers
    /// it). During a firing, an unmodelled member of a client object refuses.
    pub(crate) fn client_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        // A font question belongs to the call that asked it (`ClientState::font_refusal_pending`).
        self.client.font_refusal_pending = false;
        // Style and TextColor are value handles used by server-side mods too;
        // ChatFormatting constants are text markers produced by this surface.
        if let Some(text) = crate::game_object::text(receiver)
            && (text.starts_with("client/style/")
                || text.starts_with("client/text-color/")
                || text.starts_with("enum/net/minecraft/ChatFormatting/"))
            && (matches!(receiver, Value::Host(_))
                || text.starts_with("enum/net/minecraft/ChatFormatting/"))
        {
            if let Some(value) = text::text_call(text, receiver, r, args) {
                return Some(Ok(value));
            }
            if text.starts_with("enum/") || self.client.frame.is_none() {
                return None;
            }
            return Some(Err(NativeError::UnknownHostCall(r.clone())));
        }
        if !self.client_environment() {
            return None;
        }
        if let Some(answer) = self.screen_scope_call(r, receiver, args) {
            return Some(answer);
        }
        if let Some(value) =
            client_registrations::instance(&mut self.client.registrations, r, receiver, args)
        {
            return Some(value);
        }
        // A client registration event (`RegisterKeyMappingsEvent`...), as its firing names it.
        if let Some(event) = crate::host_game_event::fired_event(receiver) {
            return self.client_event_call(event.name(), r, args);
        }
        // A component's style calls (`client_text.rs`); an unlisted member keeps
        // the host's own component model.
        if crate::host_component::is_component(receiver) {
            return Some(match text::component_call(r, args)? {
                // The style as it is now (`withStyle`/`setStyle` change it in place).
                // Read `base` directly so unserialisable translation arguments or
                // cyclic siblings do not block `getStyle()`.
                text::ComponentCall::GetStyle => {
                    match crate::host_component::state_of(receiver, Some(&self.component_states)) {
                        Some(state) => Ok(text::style_of(&serde_json::Value::Object(state.base))),
                        None => self.unknown_host_call(r),
                    }
                }
                text::ComponentCall::Unknown => self.unknown_host_call(r),
                // `return this`, the receiver's style changed in place.
                text::ComponentCall::Restyle { codes, replace } => crate::host_component::change(
                    &mut self.component_states,
                    receiver,
                    &crate::facade_text::argument,
                    |state| text::apply_style(&mut state.base, &codes, replace),
                )
                .map(|()| receiver.clone()),
            });
        }
        // `NonNullList` is a `List`: the inventory's lists (host collections,
        // #156 P14d2) answer as one.
        if r.owner == "net/minecraft/core/NonNullList"
            && matches!(receiver, Value::Host(handle)
                if handle.kind() == crate::host_ref::HostKind::Collection)
        {
            let as_list = MethodRef {
                owner: "java/util/List".into(),
                ..r.clone()
            };
            return Some(crate::Host::invoke_instance(self, &as_list, receiver, args));
        }
        // Only a runtime-created input-key handle can enter this dispatch.
        // A jar String that happens to spell the handle marker remains a
        // String for casts, instanceof, collections and every member call.
        let input_key = match receiver {
            Value::Host(handle) if handle.kind() == crate::HostKind::GameValue => {
                input_key_parts(handle.key())
            }
            _ => None,
        };
        if let Some((mouse, code)) = input_key {
            return Some(match (r.name.as_str(), r.descriptor.as_str()) {
                ("getType", "()Lcom/mojang/blaze3d/platform/InputConstants$Type;") => {
                    Ok(marker(format!(
                        "enum/{INPUT_TYPE}/{}",
                        if mouse { "MOUSE" } else { "KEYBOARD" }
                    )))
                }
                ("getValue", "()I") => Ok(Value::Int(code)),
                ("getName", "()Ljava/lang/String;") | ("toString", "()Ljava/lang/String;") => {
                    Ok(Value::Ref(Some(input_key_name(mouse, code))))
                }
                ("equals", "(Ljava/lang/Object;)Z") => Ok(flag(
                    args.first().and_then(|value| match value {
                        Value::Host(handle) if handle.kind() == crate::HostKind::GameValue => {
                            input_key_parts(handle.key())
                        }
                        _ => None,
                    }) == Some((mouse, code)),
                )),
                ("hashCode", "()I") => Err(NativeError::UnknownHostCall(r.clone())),
                _ => return None,
            });
        }
        let text = crate::game_object::text(receiver)?.clone();
        if (text == format!("enum/{INPUT_TYPE}/KEYBOARD")
            || text == format!("enum/{INPUT_TYPE}/MOUSE"))
            && matches!(r.name.as_str(), "name" | "toString")
            && r.descriptor == "()Ljava/lang/String;"
        {
            return Some(Ok(Value::Ref(Some(
                if text.ends_with("/MOUSE") {
                    "MOUSE"
                } else {
                    "KEYBOARD"
                }
                .into(),
            ))));
        }
        if (text == format!("enum/{INPUT_TYPE}/KEYBOARD")
            || text == format!("enum/{INPUT_TYPE}/MOUSE"))
            && r.name == "getOrCreate"
            && r.descriptor == "(I)Lcom/mojang/blaze3d/platform/InputConstants$Key;"
        {
            let code = int_arg(args, 0)?;
            return Some(Ok(marker(input_key_marker(text.ends_with("/MOUSE"), code))));
        }
        // Every `client/…` and `key-category/…` object is a handle now (#156 P16d), so a `String`
        // spelling one is just that `String`: its own members answer, never the object's (#232's
        // review: `new KeyMapping("client/minecraft", ..).getName().equals(..)` in a firing).
        if !matches!(receiver, Value::Host(_))
            && (text.starts_with("client/") || text.starts_with(CATEGORY))
        {
            return None;
        }
        let refuse = || Err(NativeError::UnknownHostCall(r.clone()));
        if let Some(value) = crate::client_frame::block_pos_call(&text, r) {
            return Some(Ok(value));
        }
        if let Some(value) = crate::client_frame::hit_call(&text, r) {
            return Some(Ok(value));
        }
        // Constructors record their arguments against the new object.
        if r.name == "<init>" {
            if r.owner == KEY_MAPPING {
                if let Some(init) = key_init(&r.descriptor, args, &self.client.categories) {
                    self.client.key_inits.insert(text, init);
                }
                return Some(Ok(Value::Void));
            }
            if r.owner == KEY_CATEGORY {
                let (namespace, path) = identifier_parts(args.first()?);
                self.client
                    .categories
                    .insert(text, format!("{namespace}:{path}"));
                return Some(Ok(Value::Void));
            }
            // `new ItemStack(Items.CHEST)` in a firing: a stack of the frame.
            if r.owner == "net/minecraft/world/item/ItemStack"
                && let Some(frame) = self.client.frame.clone()
            {
                return Some(
                    self.client_make_stack(&frame, &text, args)
                        .map(|()| Value::Void)
                        .ok_or_else(|| NativeError::UnknownHostCall(r.clone())),
                );
            }
            return None;
        }
        // `BuiltInRegistries.ITEM.getKey(item)`: the item's identifier.
        if r.name == "getKey"
            && (text == "client/registry/minecraft:block_entity_type"
                || text == "registry/minecraft:block_entity_type")
            && let Some(id) = args
                .first()
                .and_then(crate::game_object::text)
                .and_then(|a| {
                    a.strip_prefix(items::ITEM)
                        .or_else(|| a.strip_prefix(items::BLOCK))
                        .or_else(|| a.strip_prefix("client/block_entity_type/"))
                })
        {
            return Some(Ok(crate::host_ids::identifier_of_text(id)));
        }
        // A stack the frame built answers as its slot marker does.
        if !text.starts_with("client/")
            && let Some(frame) = self.client.frame.clone()
            && let Some(slot) = self.client.made_aliases.get(&text).cloned()
        {
            let object = match slot {
                StackSlot::Made(n) => format!("{}made/{n}", items::STACK),
                _ => return None,
            };
            let answer = self.client_item_answer(&object, &frame, r, args).flatten();
            return Some(answer.map_or_else(refuse, Ok));
        }
        // `GameType` predicates on the enum constant `getPlayerMode` hands out.
        if let Some(mode) = text.strip_prefix(&format!("enum/{GAME_TYPE}/")) {
            let answer = match (r.name.as_str(), r.descriptor.as_str()) {
                ("isSurvival", "()Z") => matches!(mode, "SURVIVAL" | "ADVENTURE"),
                ("isCreative", "()Z") => mode == "CREATIVE",
                ("isBlockPlacingRestricted", "()Z") => matches!(mode, "ADVENTURE" | "SPECTATOR"),
                _ => return None,
            };
            return Some(Ok(flag(answer)));
        }
        if self.client.key_inits.contains_key(&text)
            && crate::jar_object::object_class(receiver) == Some(KEY_MAPPING)
        {
            return self.key_mapping_call(&text, r, args);
        }
        if self.client.categories.contains_key(&text)
            && matches!(r.name.as_str(), "equals" | "hashCode")
        {
            return Some(self.unknown_host_call(r));
        }
        // `equals`, `hashCode` and `toString` of a client handle are its
        // kind's (`game_object.rs`, `client_values.rs`), answered before here.
        if let Some(id) = text.strip_prefix(CATEGORY) {
            return Some(match r.name.as_str() {
                "id" => Ok(crate::host_ids::identifier_of_text(id)),
                "label" => Ok(component_value(
                    "translatable",
                    &format!("key.category.{}", id.replacen(':', ".", 1)),
                )),
                _ => refuse(),
            });
        }
        if !text.starts_with("client/") {
            return None;
        }
        if let Some(hex) = text.strip_prefix(AWT_COLOR) {
            let argb = u32::from_str_radix(hex, 16).ok()?;
            let channel = |shift: u32| Value::Int(((argb >> shift) & 0xff) as i32);
            return Some(match r.name.as_str() {
                "getRGB" => Ok(Value::Int(argb as i32)),
                "getAlpha" => Ok(channel(24)),
                "getRed" => Ok(channel(16)),
                "getGreen" => Ok(channel(8)),
                "getBlue" => Ok(channel(0)),
                _ => refuse(),
            });
        }
        let Some(frame) = self.client.frame.clone() else {
            // Outside a firing: what the host answered before this surface.
            return None;
        };
        if text == GRAPHICS {
            // An unanswered graphics call is refused AND recorded, like every
            // other client member below (`refuse_client_call`).
            return Some(match self.graphics_call(&frame, r, args) {
                Some(answer) => answer,
                None => Err(self.refuse_client_call(r)),
            });
        }
        if let Some(answer) = crate::level_sound::client_frame_call(self, &text, &frame, r, args) {
            return Some(answer);
        }
        let answer = self.client_answer(&text, &frame, r, args);
        Some(match answer {
            Some(value) => Ok(value),
            None => Err(self.refuse_client_call(r)),
        })
    }

    /// In a container screen's frame ([`ClientSnapshot::screen_scope`]), the one choke point
    /// for a client object's members: the `GuiGraphicsExtractor` (minus font measurement) and its
    /// pose answer; every other client handle's member is refused and counted, because the frame
    /// fills none of the facts it would answer from (key states, the player, options, level,
    /// sounds, stacks). `None` outside that scope, or for a receiver that is not a client
    /// handle (jar objects, components, identifiers keep the host's own answers).
    fn screen_scope_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let frame = self.client.frame.clone()?;
        if !frame.screen_scope {
            return None;
        }
        let key = crate::game_object::text(receiver)?.clone();
        // A registered `KeyMapping` (a jar object) answers from the frame's key presses, which
        // this frame does not fill.
        let key_mapping = self.client.key_inits.contains_key(&key)
            && crate::jar_object::object_class(receiver) == Some(KEY_MAPPING);
        let client_handle = matches!(receiver, Value::Host(_))
            && (key.starts_with("client/")
                || key.starts_with(CATEGORY)
                || input_key_parts(&key).is_some());
        if !client_handle && !key_mapping {
            return None;
        }
        Some(match key.as_str() {
            GRAPHICS => match self.graphics_call(&frame, r, args) {
                Some(answer) => answer,
                None => Err(self.refuse_client_call(r)),
            },
            POSE => match self.pose_call(r, args) {
                Some(value) => Ok(value),
                None => Err(self.refuse_client_call(r)),
            },
            // `Font.width` / `Font.split`, answered exactly or counted (`client_font.rs`).
            FONT => match self.font_call(&frame, r, args) {
                Some(value) => Ok(value),
                None => Err(self.refuse_client_call(r)),
            },
            // `Entity.level()`: the player's level is the client's one level, a handle (the
            // frame says it is present); anything asked of it is refused here in turn.
            PLAYER
                if r.name == "level"
                    && r.descriptor == "()Lnet/minecraft/world/level/Level;"
                    && frame.level.is_some() =>
            {
                Ok(marker(LEVEL))
            }
            // 26.3 `Inventory.getName` (offsets 0-3) is `DEFAULT_NAME`, and
            // `Nameable.getDisplayName` (offsets 0-6) `getName()`: a constant, no frame fact
            // (`AbstractContainerScreen.<init>` reads it for `playerInventoryTitle`).
            items::INVENTORY
                if matches!(r.name.as_str(), "getDisplayName" | "getName")
                    && r.descriptor == "()Lnet/minecraft/network/chat/Component;" =>
            {
                Ok(crate::host_component::inventory_default_name())
            }
            _ => Err(self.refuse_client_call(r)),
        })
    }

    /// A NeoForge client event object's members: the registration events'
    /// `register` calls, and the player of a login/logout event. Anything
    /// else goes to the game-event façade.
    fn client_event_call(
        &mut self,
        event: &str,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let state = &mut self.client;
        if event == GameEvent::ClientRegisterKeyMappings.name() {
            return match r.name.as_str() {
                "register" => {
                    let mapping = args.first().cloned().unwrap_or(Value::Ref(None));
                    state.register_key(&mapping, "RegisterKeyMappingsEvent.register");
                    Some(Ok(Value::Void))
                }
                // A category needs no registration of its own here: a mapping
                // names its category.
                "registerCategory" => Some(Ok(Value::Void)),
                _ => None,
            };
        }
        if event == GameEvent::ClientRegisterGuiLayers.name() {
            let source = "RegisterGuiLayersEvent";
            return match (r.name.as_str(), args.len()) {
                ("registerBelowAll" | "registerAboveAll", 2) => {
                    state.register_layer(&args[0], &args[1], source);
                    Some(Ok(Value::Void))
                }
                ("registerBelow" | "registerAbove", 3) => {
                    state.register_layer(&args[1], &args[2], source);
                    Some(Ok(Value::Void))
                }
                // Replacing or wrapping a vanilla layer needs the layer.
                _ => Some(Err(NativeError::UnknownHostCall(r.clone()))),
            };
        }
        if event == GameEvent::ClientItemTooltip.name() {
            let frame = state.frame.as_deref();
            return Some(match r.name.as_str() {
                "getToolTip" => Ok(marker(TOOLTIP_LIST)),
                "getItemStack" => Ok(tooltip_stack()),
                "getFlags" => Ok(frame
                    .map(|f| tooltip_flag(f.advanced_tooltips, f.creative_tooltips))
                    .unwrap_or_else(tooltip_flag_from_frame)),
                "getContext" => Ok(tooltip_context()),
                "getEntity" => Ok(frame
                    .and_then(|f| f.player.as_ref())
                    .map_or(Value::Ref(None), |_| marker(PLAYER))),
                _ => Err(NativeError::UnknownHostCall(r.clone())),
            });
        }
        let client_event = matches!(
            event,
            "client_join" | "client_disconnect" | "client_tick_start" | "client_tick_end"
        );
        if client_event && r.name == "getPlayer" {
            let frame = state.frame.as_deref();
            return Some(Ok(frame
                .and_then(|f| f.player.as_ref())
                .map_or(Value::Ref(None), |_| marker(PLAYER))));
        }
        None
    }

    /// `KeyMapping` members on a mapping the jar constructed.
    fn key_mapping_call(
        &mut self,
        object: &str,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let state = &mut self.client;
        let index = state.keys.iter().position(|key| key.marker == object);
        let init = state.key_inits.get(object)?.clone();
        let in_frame = state.in_frame();
        let press = index.and_then(|i| state.presses.get_mut(i));
        Some(Ok(match (r.name.as_str(), r.descriptor.as_str()) {
            ("consumeClick", "()Z") => {
                // Outside a firing nothing was pressed: the client is starting.
                let clicked = match press {
                    Some(press) if in_frame && press.clicks > 0 => {
                        press.clicks -= 1;
                        true
                    }
                    _ => false,
                };
                flag(clicked)
            }
            ("isDown", "()Z") => flag(in_frame && press.is_some_and(|p| p.down)),
            ("setDown", "(Z)V") => {
                if let Some(press) = press {
                    press.down = int_arg(args, 0).is_some_and(|v| v != 0);
                }
                Value::Void
            }
            // The jar's own `String`, never parsed into an object whatever it spells
            // (#232's review: `new KeyMapping("client/minecraft", ..)`).
            ("getName", "()Ljava/lang/String;") => text(init.name),
            ("getCategory", _)
                if init
                    .category_value
                    .as_ref()
                    .is_some_and(crate::jar_object::is_object) =>
            {
                return Some(self.unknown_host_call(r));
            }
            // Java returns the constructor argument verbatim, including null.
            ("getCategory", _) => init.category_value.clone().unwrap_or(Value::Ref(None)),
            ("hashCode", "()I") => return Some(Err(NativeError::UnknownHostCall(r.clone()))),
            ("equals", "(Ljava/lang/Object;)Z") => flag(
                args.first()
                    .filter(|value| crate::jar_object::object_class(value) == Some(KEY_MAPPING))
                    .and_then(crate::jar_object::object_key)
                    .is_some_and(|o| o == object),
            ),
            ("getDefaultKey", "()Lcom/mojang/blaze3d/platform/InputConstants$Key;") => {
                marker(init.default_key.clone())
            }
            ("getKey", "()Lcom/mojang/blaze3d/platform/InputConstants$Key;") => {
                marker(init.key.clone())
            }
            ("same", "(Lnet/minecraft/client/KeyMapping;)Z") => flag(
                args.first()
                    .filter(|value| crate::jar_object::object_class(value) == Some(KEY_MAPPING))
                    .and_then(crate::jar_object::object_key)
                    .and_then(|other| state.key_inits.get(other))
                    .is_some_and(|other| other.key == init.key),
            ),
            ("setKey", "(Lcom/mojang/blaze3d/platform/InputConstants$Key;)V") => {
                let Some(Value::Host(handle)) = args.first() else {
                    return Some(Err(NativeError::UnknownHostCall(r.clone())));
                };
                let Some((_, _)) = input_key_parts(handle.key()) else {
                    return Some(Err(NativeError::UnknownHostCall(r.clone())));
                };
                if handle.kind() != crate::HostKind::GameValue {
                    return Some(Err(NativeError::UnknownHostCall(r.clone())));
                }
                if let Some(stored) = state.key_inits.get_mut(object) {
                    stored.key = handle.key().to_owned();
                }
                Value::Void
            }
            _ if in_frame => return Some(Err(NativeError::UnknownHostCall(r.clone()))),
            _ => return None,
        }))
    }

    /// A client stand-in's member during a firing, or `None` (the caller
    /// refuses).
    fn client_answer(
        &mut self,
        object: &str,
        frame: &ClientSnapshot,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Value> {
        let name = r.name.as_str();
        let returns = return_desc(&r.descriptor);
        let int = |value: i32| Some(Value::Int(value));
        let double = |value: f64| Some(Value::Float(value));
        // A float return is rounded to f32 precision, as the interpreter's `F`.
        let float = |value: f32| Some(Value::Float(f64::from(value)));
        if let Some(value) = crate::client_frame::vec3_call(object, r, args) {
            return Some(value);
        }
        if let Some(coords) = object.strip_prefix(VEC3) {
            let [x, y, z] = crate::client_values::vec3(coords)?;
            return match (name, returns) {
                ("x", "D") => double(x),
                ("y", "D") => double(y),
                ("z", "D") => double(z),
                ("distanceTo", "D") | ("distanceToSqr", "D") => {
                    let o = crate::game_object::text(args.first()?)?.strip_prefix(VEC3)?;
                    let [ox, oy, oz] = crate::client_values::vec3(o)?;
                    let d2 = (x - ox).powi(2) + (y - oy).powi(2) + (z - oz).powi(2);
                    double(if name == "distanceTo" { d2.sqrt() } else { d2 })
                }
                _ => None,
            };
        }
        if let Some(option) = object.strip_prefix(OPTION) {
            if let Some(value) = crate::client_frame::options_call(option, &frame.options, r) {
                return Some(value);
            }
            return match (option, name) {
                ("renderDistance", "get") => {
                    Some(Value::Boxed(crate::Boxed::Integer(frame.render_distance)))
                }
                ("mainHand", "get") => Some(marker(format!(
                    "enum/net/minecraft/world/entity/HumanoidArm/{}",
                    if frame.left_handed { "LEFT" } else { "RIGHT" }
                ))),
                ("hideGui", "get") => Some(boolean_value(frame.hide_gui)),
                _ => None,
            };
        }
        if let Some(mapping) = object.strip_prefix(VANILLA_KEY) {
            return match (name, returns) {
                ("isDown", "Z") => Some(flag(frame.vanilla_keys_down.iter().any(|k| k == mapping))),
                ("getName", _) => Some(text(mapping)),
                _ => None,
            };
        }
        match object {
            MINECRAFT => match (name, returns) {
                ("getWindow", _) => Some(marker(WINDOW)),
                ("isPaused", "Z") => Some(flag(frame.paused)),
                ("isWindowActive", "Z") => Some(flag(true)),
                ("hasSingleplayerServer" | "isLocalServer" | "isSingleplayer", "Z") => {
                    Some(flag(frame.single_player))
                }
                ("getFps", "I") => int(frame.fps),
                ("getDeltaTracker", _) => Some(marker(DELTA)),
                ("getDebugOverlay", _) => Some(marker(DEBUG)),
                ("getConnection", _) => Some(if frame.level.is_some() {
                    marker(CONNECTION)
                } else {
                    Value::Ref(None)
                }),
                ("getCameraEntity", _) => Some(if frame.player.is_some() {
                    marker(PLAYER)
                } else {
                    Value::Ref(None)
                }),
                ("isSameThread", "Z") => Some(flag(true)),
                ("getSingleplayerServer", _) => Some(if frame.single_player {
                    marker(INTEGRATED_SERVER)
                } else {
                    Value::Ref(None)
                }),
                ("getTextureManager", _) => Some(marker(TEXTURE_MANAGER)),
                ("hitResult", _) => frame
                    .hit_result
                    .as_ref()
                    .map(crate::client_frame::hit_value),
                ("getUser", _) if frame.player.is_some() => Some(marker(USER)),
                ("execute", "V") | ("schedule" | "submit", _) => {
                    let task = args.first()?.clone();
                    if self.client.tasks.len() >= MAX_TASKS {
                        return None;
                    }
                    self.client.tasks.push(task);
                    // `submit` answers a future; nothing waits on it here.
                    Some(if returns == "V" {
                        Value::Void
                    } else {
                        Value::Ref(None)
                    })
                }
                // `getCurrentServer` is null in single player and out of a
                // world; a server's address and name are not in the façade.
                ("getCurrentServer", _) if frame.single_player || frame.level.is_none() => {
                    Some(Value::Ref(None))
                }
                _ => None,
            },
            OPTIONS => match name {
                "renderDistance" | "mainHand" | "particles" | "sensitivity" => {
                    Some(marker(format!("{OPTION}{name}")))
                }
                _ => None,
            },
            WINDOW => match (name, returns) {
                ("getGuiScaledWidth", "I") => int(frame.gui_size[0]),
                ("getGuiScaledHeight", "I") => int(frame.gui_size[1]),
                ("getWidth" | "getScreenWidth", "I") => int(frame.window_size[0]),
                ("getHeight" | "getScreenHeight", "I") => int(frame.window_size[1]),
                ("getGuiScale", "I") => frame.gui_scale.map(Value::Int),
                _ => None,
            },
            GUI | HUD => match (name, returns) {
                ("getChat", _) => Some(marker(CHAT)),
                ("getGuiTicks", "I") => int(frame.gui_ticks),
                ("isHidden", "Z") => Some(flag(frame.hide_gui)),
                // 26.3 reads the open screen through the GUI.
                ("screen", _) if !frame.screen_open => Some(Value::Ref(None)),
                ("setOverlayMessage", "V") => {
                    let text = text_arg(args.first()?, &self.component_states)?;
                    self.client.message(ClientMessage::Overlay(text));
                    Some(Value::Void)
                }
                _ => None,
            },
            CONNECTION => match (name, returns) {
                ("getLevel", _) if frame.level.is_some() => Some(marker(LEVEL)),
                ("getServerData", _) if frame.single_player => Some(Value::Ref(None)),
                _ => None,
            },
            CHAT => match (name, r.descriptor.as_str()) {
                ("addMessage", "(Lnet/minecraft/network/chat/Component;)V") => {
                    let text = text_arg(args.first()?, &self.component_states)?;
                    self.client.message(ClientMessage::Chat(text));
                    Some(Value::Void)
                }
                _ => None,
            },
            PLAYER => {
                let p = frame.player.as_ref()?;
                if name == "blockPosition" {
                    return Some(crate::client_frame::block_pos_value(
                        p.pos.map(crate::client_frame::floor),
                    ));
                }
                if name == "getBlockX" {
                    return int(crate::client_frame::floor(p.pos[0]));
                }
                if name == "getBlockY" {
                    return int(crate::client_frame::floor(p.pos[1]));
                }
                if name == "getBlockZ" {
                    return int(crate::client_frame::floor(p.pos[2]));
                }
                if name == "isUsingItem" {
                    return p.using_item.map(flag);
                }
                if name == "isSleeping" {
                    return p.sleeping.map(flag);
                }
                if let Some(value) = self.client_player_items(frame, name, returns, args) {
                    return Some(value);
                }
                match (name, returns) {
                    ("displayClientMessage", "V") => {
                        let text = text_arg(args.first()?, &self.component_states)?;
                        let overlay = int_arg(args, 1).is_some_and(|v| v != 0);
                        self.client.message(if overlay {
                            ClientMessage::Overlay(text)
                        } else {
                            ClientMessage::Chat(text)
                        });
                        Some(Value::Void)
                    }
                    ("sendSystemMessage", "V") => {
                        let text = text_arg(args.first()?, &self.component_states)?;
                        self.client.message(ClientMessage::Chat(text));
                        Some(Value::Void)
                    }
                    ("position", _) => Some(vec3(p.pos)),
                    ("blockPosition", "()Lnet/minecraft/core/BlockPos;") => Some(
                        crate::client_frame::block_pos_value(p.pos.map(crate::client_frame::floor)),
                    ),
                    ("getBlockX", "()I") => int(crate::client_frame::floor(p.pos[0])),
                    ("getBlockY", "()I") => int(crate::client_frame::floor(p.pos[1])),
                    ("getBlockZ", "()I") => int(crate::client_frame::floor(p.pos[2])),
                    ("isUsingItem", "()Z") => p.using_item.map(flag),
                    ("isSleeping", "()Z") => p.sleeping.map(flag),
                    ("getEyeY", "D") => double(p.pos[1] + if p.sneaking { 1.27 } else { 1.62 }),
                    ("getX", "D") => double(p.pos[0]),
                    ("getY", "D") => double(p.pos[1]),
                    ("getZ", "D") => double(p.pos[2]),
                    ("getYRot", "F") => float(p.yaw),
                    ("getXRot", "F") => float(p.pitch),
                    ("getHealth", "F") => float(p.health),
                    ("getMaxHealth", "F") => float(p.max_health),
                    ("isCreative", "Z") => Some(flag(p.creative)),
                    ("isSpectator", "Z") => Some(flag(p.spectator)),
                    ("isShiftKeyDown" | "isCrouching", "Z") => Some(flag(p.sneaking)),
                    ("isSprinting", "Z") => Some(flag(p.sprinting)),
                    ("onGround", "Z") => Some(flag(p.on_ground)),
                    ("isFallFlying", "Z") => Some(flag(p.fall_flying)),
                    ("isAlive", "Z") => Some(flag(p.health > 0.0)),
                    ("getId", "I") => int(p.entity_id),
                    ("getName" | "getDisplayName", _) => Some(component_value("literal", &p.name)),
                    ("getScoreboardName" | "getPlainTextName", _) => Some(text(p.name.clone())),
                    ("getUUID", _) => uuid_value(&uuid_text(p.uuid)).ok(),
                    ("getStringUUID", _) => Some(text(uuid_text(p.uuid))),
                    ("level" | "clientLevel", _) if frame.level.is_some() => Some(marker(LEVEL)),
                    _ => None,
                }
            }
            LEVEL => {
                let level = frame.level.as_ref()?;
                match (name, returns) {
                    ("getGameTime", "J") => Some(Value::Long(level.game_time)),
                    // 26.3 names the day clock after the overworld.
                    ("getDayTime" | "getOverworldClockTime", "J") => {
                        Some(Value::Long(level.day_time))
                    }
                    ("dimension", _) => Some(crate::host_ids::resource_key_named(
                        "minecraft:dimension",
                        &level.dimension,
                    )),
                    ("isClientSide", "Z") => Some(flag(true)),
                    ("getMinY", "I") => int(level.min_y),
                    ("getHeight", "I") => int(level.height),
                    ("getMaxY", "I") => int(level.min_y + level.height - 1),
                    _ => None,
                }
            }
            GAME_RENDERER => match (name, returns) {
                ("getMainCamera" | "mainCamera", _) if frame.camera.is_some() => {
                    Some(marker(CAMERA))
                }
                _ => None,
            },
            CAMERA => {
                let camera = frame.camera.as_ref()?;
                match (name, returns) {
                    ("position" | "getPosition", _) => Some(vec3(camera.pos)),
                    ("xRot" | "getXRot", "F") => float(camera.pitch),
                    ("yRot" | "getYRot" | "yaw", "F") => float(camera.yaw),
                    ("isDetached", "Z") => Some(flag(camera.detached)),
                    ("isInitialized", "Z") => Some(flag(true)),
                    ("entity" | "getEntity", _) => Some(if frame.player.is_some() {
                        marker(PLAYER)
                    } else {
                        Value::Ref(None)
                    }),
                    _ => None,
                }
            }
            // Naming a texture is free; registering or uploading one is not
            // modelled, and refuses.
            TEXTURE_MANAGER => match (name, returns) {
                ("getTexture", _) => {
                    let (namespace, path) = identifier_parts(args.first()?);
                    Some(marker(format!("client/texture/{namespace}:{path}")))
                }
                ("release", "V") => Some(Value::Void),
                _ => None,
            },
            INTEGRATED_SERVER => match (name, returns) {
                ("isDedicatedServer", "Z") => Some(flag(false)),
                ("isSingleplayer", "Z") => Some(flag(true)),
                ("isPublished", "Z") => Some(flag(frame.lan_open)),
                _ => None,
            },
            USER => {
                let p = frame.player.as_ref()?;
                match (name, returns) {
                    ("getName", _) => Some(marker(p.name.clone())),
                    ("getProfileId", _) => uuid_value(&uuid_text(p.uuid)).ok(),
                    _ => None,
                }
            }
            GAME_MODE => match (name, returns) {
                ("getPlayerMode", _) => {
                    let mode = &frame.player.as_ref()?.game_mode;
                    Some(marker(format!("enum/{GAME_TYPE}/{mode}")))
                }
                _ => None,
            },
            DEBUG => match (name, returns) {
                // `getDebugOverlay().showDebugScreen()` and 26.3's
                // `debugEntries.isOverlayVisible()`.
                ("showDebugScreen" | "isOverlayVisible", "Z") => Some(flag(frame.debug_screen)),
                _ => None,
            },
            FONT => self.font_call(frame, r, args),
            DELTA => match (name, returns) {
                ("getGameTimeDeltaPartialTick", "F") => float(frame.partial_tick),
                ("getGameTimeDeltaTicks" | "getRealtimeDeltaTicks", "F") => float(1.0),
                _ => None,
            },
            POSE => self.pose_call(r, args),
            _ => self.client_item_answer(object, frame, r, args).flatten(),
        }
    }

    /// Resolve a `GuiGraphicsExtractor.text` / `centeredText` text argument.
    /// In Minecraft 26.3, the `String` overloads return early on `null`
    /// (drawing nothing), whereas the `Component` overloads immediately invoke
    /// `Component.getVisualOrderText()` and throw `NullPointerException`.
    ///
    /// `param` is the `Component` parameter's name in the real method's
    /// LocalVariableTable, which Java 25's helpful NPE message quotes: `str` in
    /// both `text(Font, Component, …)` overloads, `text` in
    /// `centeredText(Font, Component, …)` (javap -l of the 26.3 client jar).
    fn graphics_text_arg(
        &self,
        arg: &Value,
        descriptor: &str,
        param: &str,
    ) -> Result<Option<ClientText>, Option<NativeError>> {
        if let Some(text) = text_arg(arg, &self.component_states) {
            return Ok(Some(text));
        }
        if *arg == Value::Ref(None) {
            if descriptor.starts_with("(Lnet/minecraft/client/gui/Font;Ljava/lang/String;") {
                return Ok(None);
            }
            if descriptor.starts_with(
                "(Lnet/minecraft/client/gui/Font;Lnet/minecraft/network/chat/Component;",
            ) {
                return Err(Some(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some(format!(
                        r#"Cannot invoke "net.minecraft.network.chat.Component.getVisualOrderText()" because "{param}" is null"#
                    )),
                    frame: None,
                }));
            }
        }
        Err(None)
    }

    /// `GuiGraphicsExtractor` members: drawing records a [`DrawOp`]; one past
    /// [`MAX_DRAWS_PER_CALL`] refuses the callback.
    fn graphics_call(
        &mut self,
        frame: &ClientSnapshot,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let ints = |from: usize, n: usize| -> Option<Vec<i32>> {
            (from..from + n).map(|at| int_arg(args, at)).collect()
        };
        let drawn = match (r.name.as_str(), r.descriptor.as_str()) {
            ("guiWidth", "()I") => return Some(Ok(Value::Int(frame.gui_size[0]))),
            ("guiHeight", "()I") => return Some(Ok(Value::Int(frame.gui_size[1]))),
            ("pose", _) => return Some(Ok(marker(POSE))),
            ("fill", "(IIIII)V") => {
                let v = ints(0, 5)?;
                self.client.fill([v[0], v[1], v[2], v[3]], v[4] as u32)
            }
            ("fillGradient", "(IIIIII)V") => {
                let v = ints(0, 6)?;
                let rect = self.client.posed_rect([v[0], v[1], v[2], v[3]]);
                self.client.draw(DrawOp::Gradient {
                    rect,
                    top: v[4] as u32,
                    bottom: v[5] as u32,
                })
            }
            // GuiGraphicsExtractor.outline: four one-pixel fills.
            ("outline", "(IIIII)V") => {
                let v = ints(0, 5)?;
                let (x, y, w, h, c) = (v[0], v[1], v[2], v[3], v[4] as u32);
                self.client
                    .fill([x, y, x + w, y + 1], c)
                    .and_then(|()| self.client.fill([x, y + h - 1, x + w, y + h], c))
                    .and_then(|()| self.client.fill([x, y + 1, x + 1, y + h - 1], c))
                    .and_then(|()| self.client.fill([x + w - 1, y + 1, x + w, y + h - 1], c))
            }
            // `text(font, line, x, y, colour[, shadow])` and `centeredText(font, line, x, y,
            // colour)` of a `Font.split` line (`client_font.rs`): its text, drawn as the
            // component overloads draw theirs. 26.3's `centeredText` is `text(font, line,
            // x - font.width(line) / 2, y, colour)`, Java's integer division.
            ("text" | "centeredText", descriptor)
                if descriptor.starts_with(FONT_CHAR_SEQUENCE_ARGS) =>
            {
                let text = self.char_sequence_text(args.get(1)?)?;
                let v = ints(2, 3)?;
                let shadow = int_arg(args, 5).is_none_or(|v| v != 0);
                let x = if r.name == "centeredText" {
                    v[0] - self.frame_width(frame, text.clone(), font::WidthOrder::Visual)? / 2
                } else {
                    v[0]
                };
                self.client.text(text, x, v[1], v[2] as u32, shadow)
            }
            // 26.3 `textWithWordWrap(font, text, x, y, width, colour[, shadow])` (offsets 0-67):
            // each line of `font.split(text, width)` drawn with `text(font, line, x, y, colour,
            // shadow)` at `y` steps of `font.lineHeight` (9); answers the `y` after the last.
            // The six-argument overload draws with a shadow. Only the surface's own font
            // splits here.
            ("textWithWordWrap", descriptor)
                if descriptor == WORD_WRAP || descriptor == WORD_WRAP_SHADOW =>
            {
                if crate::game_object::handle_key(args.first()?) != Some(FONT) {
                    return None;
                }
                let text = self.component_text(args.get(1)?)?;
                let v = ints(2, 4)?;
                let shadow = int_arg(args, 6).is_none_or(|v| v != 0);
                let lines = self.frame_split(frame, text, v[2])?;
                let mut y = v[1];
                for line in lines {
                    self.client.text(line, v[0], y, v[3] as u32, shadow).ok()?;
                    y = y.wrapping_add(9);
                }
                return Some(Ok(Value::Int(y)));
            }
            ("text", descriptor) if descriptor.starts_with("(Lnet/minecraft/client/gui/Font;") => {
                let arg = args.get(1)?;
                let text = match self.graphics_text_arg(arg, descriptor, "str") {
                    Ok(Some(text)) => text,
                    Ok(None) => return Some(Ok(Value::Void)),
                    Err(Some(err)) => return Some(Err(err)),
                    Err(None) => return None,
                };
                let v = ints(2, 3)?;
                // The five-argument overloads draw with a shadow.
                let shadow = int_arg(args, 5).is_none_or(|v| v != 0);
                self.client.text(text, v[0], v[1], v[2] as u32, shadow)
            }
            ("horizontalLine", "(IIII)V") => {
                let v = ints(0, 4)?;
                let (x0, x1) = (v[0].min(v[1]), v[0].max(v[1]));
                self.client.fill([x0, v[2], x1 + 1, v[2] + 1], v[3] as u32)
            }
            ("verticalLine", "(IIII)V") => {
                let v = ints(0, 4)?;
                let (y0, y1) = (v[1].min(v[2]), v[1].max(v[2]));
                self.client.fill([v[0], y0 + 1, v[0] + 1, y1], v[3] as u32)
            }
            // A fill through an explicit pipeline (`RenderPipelines.GUI`). This descriptor is
            // the pre-26.3 `com.mojang.blaze3d.pipeline` one; 26.3's `RenderPipeline` is
            // `com.mojang.renderpearl.api.pipeline.RenderPipeline`, so a 26.3 jar's call never
            // matches and stays counted (the arm's `_ => None`). Moving it to the 26.3 package
            // needs the pipeline argument checked (only `GUI` fills like this) and a fixture;
            // until then it stays counted rather than half-ported.
            ("fill", "(Lcom/mojang/blaze3d/pipeline/RenderPipeline;IIIII)V") => {
                let v = ints(1, 5)?;
                self.client.fill([v[0], v[1], v[2], v[3]], v[4] as u32)
            }
            ("item" | "fakeItem", descriptor)
                if descriptor.starts_with("(Lnet/minecraft/world/item/ItemStack;II") =>
            {
                let v = ints(1, 2)?;
                let seed = int_arg(args, 3).unwrap_or(0);
                return self
                    .any_item_draw(frame, args.first()?, [v[0], v[1]], Some(seed), None)
                    .map(|()| Ok(Value::Void));
            }
            // `item(owner, stack, x, y, seed)`: the owner only seeds animation.
            ("item", "(Lnet/minecraft/world/entity/LivingEntity;Lnet/minecraft/world/item/ItemStack;III)V") => {
                let v = ints(2, 3)?;
                return self
                    .any_item_draw(frame, args.get(1)?, [v[0], v[1]], Some(v[2]), None)
                    .map(|()| Ok(Value::Void));
            }
            ("itemDecorations", descriptor)
                if descriptor.starts_with("(Lnet/minecraft/client/gui/Font;Lnet/minecraft/world/item/ItemStack;II") =>
            {
                let stack = args.get(1)?;
                let v = ints(2, 2)?;
                // The count text is a `String` (`(Font, ItemStack, II, String)`): the jar's string
                // itself, uncut (`font::string_text` refuses one past the surface's bound, whose
                // drawn text would be a cut one), and `null` for the stack's own count.
                let count = match args.get(4) {
                    Some(Value::Ref(None)) => Some(None),
                    Some(arg) => match font::string_text(arg)? {
                        ClientText::Literal(text) => Some(Some(text)),
                        _ => return None,
                    },
                    None => None,
                };
                return self
                    .any_item_draw(frame, stack, [v[0], v[1]], None, Some(count.flatten()))
                    .map(|()| Ok(Value::Void));
            }
            // 26.3 `blitSprite(pipeline, sprite, x, y, w, h[, colour | alpha])` (offsets 0-166):
            // the six-int overload is the colour one with `-1`, the alpha one with
            // `ARGB.white(alpha)`; that looks the sprite up in the GUI atlas and draws it as its
            // `gui.scaling` says, which `DrawOp::Sprite` asks the embedder to do. Only on
            // `RenderPipelines.GUI_TEXTURED` (Sunrise's `Blend::Textured`); another pipeline
            // counts. A zero width or height draws nothing in every scaling (`Stretch` returns at
            // offsets 0-10 of the `TextureAtlasSprite` overload, `Tile` at 0-10 of
            // `blitTiledSprite`, and a `NineSlice`'s pieces are all empty), so none is recorded; a
            // negative one draws differently per scaling, which the surface doesn't know: counted.
            // A scaled pose would scale a nine-slice's borders, which a posed `rect` doesn't:
            // counted. The 10- and 11-int partial overloads (a sub-region) are not modelled.
            ("blitSprite", descriptor) if blit_sprite_overload(descriptor).is_some() => {
                if crate::game_object::handle_key(args.first()?) != Some(GUI_TEXTURED_PIPELINE) {
                    return None;
                }
                let id = crate::host_ids::identifier_key(args.get(1)?)?;
                let (namespace, path) = id.split_once(':')?;
                let v = ints(2, 4)?;
                let argb = match blit_sprite_overload(descriptor)? {
                    SpriteTint::White => u32::MAX,
                    SpriteTint::Colour => int_arg(args, 6)? as u32,
                    SpriteTint::Alpha => argb_white(float_arg(args, 6)?),
                };
                let (w, h) = (v[2], v[3]);
                if w == 0 || h == 0 {
                    return Some(Ok(Value::Void));
                }
                let [sx, sy, _, _] = self.client.pose();
                if w < 0 || h < 0 || sx != 1.0 || sy != 1.0 {
                    return None;
                }
                // A GUI sprite is named by its path under `textures/gui/sprites`; a mod's keeps
                // its namespace.
                let sprite = if namespace == "minecraft" {
                    path.to_owned()
                } else {
                    id.to_owned()
                };
                let rect = self.client.posed_rect([
                    v[0],
                    v[1],
                    v[0].wrapping_add(w),
                    v[1].wrapping_add(h),
                ]);
                self.client.draw(DrawOp::Sprite { sprite, rect, argb })
            }
            // `blitSprite(pipeline, sprite, x, y, w, h[, colour | alpha])` in the pre-26.3
            // package, as the pipeline `fill` above (a jar built against an older game).
            ("blitSprite", descriptor)
                if descriptor.starts_with(
                    "(Lcom/mojang/blaze3d/pipeline/RenderPipeline;Lnet/minecraft/resources/Identifier;IIII",
                ) =>
            {
                let (namespace, path) = identifier_parts(args.get(1)?);
                let v = ints(2, 4)?;
                let argb = match (args.get(6), descriptor.contains("IIIIF)")) {
                    (Some(alpha), true) => {
                        let alpha = float_arg(std::slice::from_ref(alpha), 0)?;
                        ((alpha.clamp(0.0, 1.0) * 255.0) as u32) << 24 | 0x00ff_ffff
                    }
                    (Some(Value::Int(colour)), false) => *colour as u32,
                    _ => u32::MAX,
                };
                // A GUI sprite is named by its path under `textures/gui/sprites`;
                // a mod's keeps its namespace.
                let sprite = if namespace == "minecraft" {
                    path.to_owned()
                } else {
                    format!("{namespace}:{path}")
                };
                let rect = self.client.posed_rect([v[0], v[1], v[0] + v[2], v[1] + v[3]]);
                self.client.draw(DrawOp::Sprite { sprite, rect, argb })
            }
            // 26.3 GuiGraphicsExtractor.blit(RenderPipeline, Identifier, int, int, float, float, int, int, int, int)
            // offsets 0-25 delegate to 12-arg (regionWidth=width, regionHeight=height),
            // which at offsets 0-26 delegates to 13-arg with color = -1 (0xFFFFFFFF),
            // which at offsets 0-56 calls innerBlit with x0=x, x1=x+w, y0=y, y1=y+h and float u/v.
            ("blit", "(Lcom/mojang/renderpearl/api/pipeline/RenderPipeline;Lnet/minecraft/resources/Identifier;IIFFIIII)V") => {
                if crate::game_object::handle_key(args.first()?) != Some(GUI_TEXTURED_PIPELINE) {
                    return None;
                }
                let texture = crate::host_ids::identifier_key(args.get(1)?)?.to_owned();
                let x = int_arg(args, 2)?;
                let y = int_arg(args, 3)?;
                let u = float_arg(args, 4)?;
                let v = float_arg(args, 5)?;
                if !u.is_finite() || !v.is_finite() {
                    return None;
                }
                let w = int_arg(args, 6)?;
                let h = int_arg(args, 7)?;
                let tex_w = int_arg(args, 8)?;
                let tex_h = int_arg(args, 9)?;
                if tex_w <= 0 || tex_h <= 0 {
                    return None;
                }
                let rect = self.client.posed_rect([x, y, x.wrapping_add(w), y.wrapping_add(h)]);
                self.client.draw(DrawOp::Blit {
                    texture,
                    src: [u, v, w as f32, h as f32],
                    dst: rect,
                    tex_size: [tex_w, tex_h],
                    tint: u32::MAX,
                })
            }
            ("centeredText", descriptor)
                if descriptor.starts_with("(Lnet/minecraft/client/gui/Font;") =>
            {
                let arg = args.get(1)?;
                let text = match self.graphics_text_arg(arg, descriptor, "text") {
                    Ok(Some(text)) => text,
                    Ok(None) => return Some(Ok(Value::Void)),
                    Err(Some(err)) => return Some(Err(err)),
                    Err(None) => return None,
                };
                let v = ints(2, 3)?;
                // `x - font.width(text) / 2`, Java's integer division, the width exact
                // (`client_font.rs`): `width(String)` sums the string in its own order; the
                // component overload measures `getVisualOrderText()`.
                let (measured, order) = if descriptor
                    .starts_with("(Lnet/minecraft/client/gui/Font;Ljava/lang/String;")
                {
                    (font::string_text(arg)?, font::WidthOrder::Logical)
                } else {
                    (self.component_text(arg)?, font::WidthOrder::Visual)
                };
                let width = self.frame_width(frame, measured, order)?;
                self.client.text(text, v[0] - width / 2, v[1], v[2] as u32, true)
            }
            _ => return None,
        };
        drawn.ok().map(|()| Ok(Value::Void))
    }

    /// An item draw of a frame stack ([`Self::client_item_draw`]) or of a runtime `ItemStack`
    /// ([`DrawOp::Stack`]); `model` is the model draw's seed, `None` for decorations. `None`
    /// (counted) for any other value, a runtime stack whose components can't all cross the
    /// boundary, or a draw past [`MAX_DRAWS_PER_CALL`].
    fn any_item_draw(
        &mut self,
        frame: &ClientSnapshot,
        stack: &Value,
        [x, y]: [i32; 2],
        model: Option<i32>,
        decorations: Option<Option<String>>,
    ) -> Option<()> {
        if let Some(slot) = self.client_stack_slot(stack) {
            return self.client_item_draw(frame, slot, [x, y], model.is_some(), decorations);
        }
        let parts = self.item_stack_components(stack)?;
        // `item` and `itemDecorations` draw nothing for an empty stack (`ItemStack.isEmpty`: air
        // or a count of 0 or less).
        if parts.count <= 0 || parts.item_id == "minecraft:air" {
            return Some(());
        }
        let unsupported = parts
            .patch
            .iter()
            .any(|(_, value)| matches!(value, Some(crate::RuntimeComponentValue::Unsupported)));
        if parts.opaque_components || unsupported {
            return None;
        }
        let [sx, sy, tx, ty] = self.client.pose();
        self.client
            .draw(DrawOp::Stack {
                stack: parts,
                at: [x as f32 * sx + tx, y as f32 * sy + ty],
                scale: [sx, sy],
                model: model.is_some(),
                seed: model.unwrap_or(0),
                decorations,
            })
            .ok()
    }

    /// Records one item draw; `None` past [`MAX_DRAWS_PER_CALL`].
    fn client_item_draw(
        &mut self,
        frame: &ClientSnapshot,
        slot: StackSlot,
        [x, y]: [i32; 2],
        model: bool,
        decorations: Option<Option<String>>,
    ) -> Option<()> {
        let stack = self.client_stack(frame, &slot);
        // GuiGraphicsExtractor.item and itemDecorations draw nothing for an
        // empty stack.
        if stack.is_empty() {
            return Some(());
        }
        let [sx, sy, tx, ty] = self.client.pose();
        let made = matches!(slot, StackSlot::Made(_) | StackSlot::Empty);
        self.client
            .draw(DrawOp::Item {
                stack,
                slot: (!made).then_some(slot),
                at: [x as f32 * sx + tx, y as f32 * sy + ty],
                scale: [sx, sy],
                model,
                decorations,
            })
            .ok()
    }

    /// `Matrix3x2fStack` members the pose supports: push, pop, translate and
    /// scale. A rotation (or anything else) refuses.
    fn pose_call(&mut self, r: &MethodRef, args: &[Value]) -> Option<Value> {
        let state = &mut self.client;
        let [sx, sy, tx, ty] = state.pose();
        let returns = return_desc(&r.descriptor);
        let this = || {
            if returns == "V" {
                Value::Void
            } else {
                marker(POSE)
            }
        };
        match (r.name.as_str(), r.descriptor.split(')').next()?) {
            ("pushMatrix", "(") => {
                if state.poses.len() >= MAX_POSE_DEPTH {
                    return None;
                }
                state.poses.push([sx, sy, tx, ty]);
                Some(this())
            }
            ("popMatrix", "(") => {
                state.poses.pop();
                Some(this())
            }
            // JOML's `translate(x, y, dest)` writing into the stack itself is
            // the two-argument form; into another matrix it is not modelled.
            ("translate" | "scale", "(FFLorg/joml/Matrix3x2f;")
                if args.get(2) != Some(&marker(POSE)) =>
            {
                None
            }
            ("translate", "(FF" | "(FFLorg/joml/Matrix3x2f;") => {
                let (dx, dy) = (float_arg(args, 0)?, float_arg(args, 1)?);
                state.set_pose([sx, sy, tx + sx * dx, ty + sy * dy]);
                Some(this())
            }
            ("scale", "(FF" | "(FFLorg/joml/Matrix3x2f;") => {
                let (a, b) = (float_arg(args, 0)?, float_arg(args, 1)?);
                state.set_pose([sx * a, sy * b, tx, ty]);
                Some(this())
            }
            ("scale", "(F") => {
                let a = float_arg(args, 0)?;
                state.set_pose([sx * a, sy * a, tx, ty]);
                Some(this())
            }
            _ => None,
        }
    }
}

impl ClientState {
    /// Replace the current pose (the stack's top, or the base one).
    fn set_pose(&mut self, pose: Pose) {
        match self.poses.last_mut() {
            Some(top) => *top = pose,
            None => self.poses.push(pose),
        }
    }
}

/// A position the façade hands out as a `Vec3`.
fn vec3([x, y, z]: [f64; 3]) -> Value {
    marker(format!(
        "{VEC3}{},{},{}",
        crate::client_values::coord_text(x),
        crate::client_values::coord_text(y),
        crate::client_values::coord_text(z),
    ))
}

fn uuid_text(uuid: u128) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (uuid >> 96) as u32,
        (uuid >> 80) as u16,
        (uuid >> 64) as u16,
        (uuid >> 48) as u16,
        uuid & 0xffff_ffff_ffff
    )
}

impl StdHost {
    /// Instance calls on dedicated tooltip handles (`TooltipConsumer`,
    /// `TooltipContext`, `TooltipDisplay`, `TooltipFlag`). `Object` methods
    /// (`toString`, `hashCode`, `equals`, `getClass`) are handled and counted
    /// earlier via `host_ref.rs`.
    pub(crate) fn tooltip_handle_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let Value::Host(handle) = receiver else {
            return None;
        };
        if matches!(
            (r.name.as_str(), r.descriptor.as_str()),
            ("toString", "()Ljava/lang/String;")
                | ("hashCode", "()I")
                | ("equals", "(Ljava/lang/Object;)Z")
        ) {
            return None;
        }
        match handle.kind() {
            HostKind::TooltipConsumer => {
                if r.name == "accept" && r.descriptor == "(Ljava/lang/Object;)V" {
                    let Some(value) = args.first().cloned() else {
                        return Some(self.unknown_host_call(r));
                    };
                    let Some(lines) = self.tooltip_sinks.get_mut(handle.key()) else {
                        return Some(self.unknown_host_call(r));
                    };
                    lines.push(value);
                    return Some(Ok(Value::Void));
                }
                Some(self.unknown_host_call(r))
            }
            HostKind::TooltipContext => {
                if handle.key() == "empty" {
                    return Some(match (r.name.as_str(), r.descriptor.as_str()) {
                        ("tickRate", "()F") => Ok(Value::Float(20.0)),
                        ("registries", "()Lnet/minecraft/core/HolderLookup$Provider;") => {
                            Ok(Value::Ref(None))
                        }
                        (
                            "mapData",
                            "(Lnet/minecraft/world/level/saveddata/maps/MapId;)Lnet/minecraft/world/level/saveddata/maps/MapItemSavedData;",
                        ) => Ok(Value::Ref(None)),
                        ("isPeaceful", "()Z") => Ok(flag(false)),
                        _ => self.unknown_host_call(r),
                    });
                }
                if r.name == "tickRate"
                    && r.descriptor == "()F"
                    && let Some(rate) = self.client.frame.as_ref().and_then(|f| f.tick_rate)
                {
                    return Some(Ok(Value::Float(f64::from(rate))));
                }
                Some(self.unknown_host_call(r))
            }
            HostKind::TooltipDisplay => {
                // Keep the legacy `"default"` opaque placeholder counted for existing callers.
                if handle.key() == "default" {
                    return Some(self.unknown_host_call(r));
                }
                let td = if handle.key() == crate::recipe_models::TOOLTIP_DISPLAY_CANONICAL_DEFAULT
                {
                    crate::recipe_models::TooltipDisplayData {
                        hide_tooltip: false,
                        hidden_components: Vec::new(),
                        hidden_known: true,
                    }
                } else if let Some(data) = self.recipes.tooltip_displays.get(handle.key()) {
                    data.clone()
                } else {
                    return Some(self.unknown_host_call(r));
                };
                match (r.name.as_str(), r.descriptor.as_str()) {
                    ("hideTooltip", "()Z") => Some(Ok(flag(td.hide_tooltip))),
                    ("shows", "(Lnet/minecraft/core/component/DataComponentType;)Z") => {
                        if td.hide_tooltip {
                            return Some(Ok(flag(false)));
                        }
                        let Some(arg) = args.first() else {
                            return Some(self.unknown_host_call(r));
                        };
                        if matches!(arg, Value::Ref(None)) {
                            // Verified on Java 25 MC 26.3: `ReferenceLinkedOpenHashSet.contains(null)`
                            // is `false`, so `shows(null)` returns `true` when `!hideTooltip`.
                            return Some(Ok(flag(true)));
                        }
                        let Some(comp_id) =
                            crate::recipe_models::resolve_data_component_type_id(self, arg)
                        else {
                            return Some(self.unknown_host_call(r));
                        };
                        if td.hidden_components.iter().any(|id| id == &comp_id) {
                            Some(Ok(flag(false)))
                        } else if td.hidden_known {
                            Some(Ok(flag(true)))
                        } else {
                            Some(self.unknown_host_call(r))
                        }
                    }
                    (
                        "withHidden",
                        "(Lnet/minecraft/core/component/DataComponentType;Z)Lnet/minecraft/world/item/component/TooltipDisplay;",
                    ) => {
                        if !td.hidden_known {
                            return Some(self.unknown_host_call(r));
                        }
                        let (Some(arg), Some(Value::Int(hide_raw))) = (args.first(), args.get(1))
                        else {
                            return Some(self.unknown_host_call(r));
                        };
                        let hide = *hide_raw != 0;
                        let Some(comp_id) =
                            crate::recipe_models::resolve_data_component_type_id(self, arg)
                        else {
                            return Some(self.unknown_host_call(r));
                        };
                        let already_hidden = td.hidden_components.iter().any(|id| id == &comp_id);
                        if already_hidden == hide {
                            return Some(Ok(receiver.clone()));
                        }
                        let mut next = td;
                        if hide {
                            next.hidden_components.push(comp_id);
                        } else {
                            next.hidden_components.retain(|id| id != &comp_id);
                        }
                        Some(Ok(self.recipes.alloc_tooltip_display(next)))
                    }
                    _ => Some(self.unknown_host_call(r)),
                }
            }
            HostKind::TooltipFlag => {
                let flags = (handle.key() == "frame")
                    .then(|| {
                        self.client
                            .frame
                            .as_ref()
                            .map(|f| (f.advanced_tooltips, f.creative_tooltips))
                    })
                    .flatten();
                let Some((advanced, creative)) = flags else {
                    return Some(self.unknown_host_call(r));
                };
                match (r.name.as_str(), r.descriptor.as_str()) {
                    ("isAdvanced", "()Z") => Some(Ok(flag(advanced))),
                    ("isCreative", "()Z") => Some(Ok(flag(creative))),
                    _ => Some(self.unknown_host_call(r)),
                }
            }
            HostKind::TooltipFlagDefault => {
                let Some((advanced, creative)) = parse_tooltip_flag_default(handle.key()) else {
                    return Some(self.unknown_host_call(r));
                };
                match (r.name.as_str(), r.descriptor.as_str()) {
                    ("isAdvanced" | "advanced", "()Z") => Some(Ok(flag(advanced))),
                    ("isCreative" | "creative", "()Z") => Some(Ok(flag(creative))),
                    ("asCreative", "()Lnet/minecraft/world/item/TooltipFlag$Default;") => {
                        Some(Ok(tooltip_flag_default_new(advanced, true)))
                    }
                    _ => Some(self.unknown_host_call(r)),
                }
            }
            _ => None,
        }
    }
}

impl NativeJar {
    /// Run an item's own 26.3 `appendHoverText` implementation. The caller
    /// supplies the item object and the real frame-equivalent arguments; this
    /// helper supplies an opaque `Consumer<Component>` sink, preserving
    /// translatable components and their styles for Sunrise to resolve.
    ///
    /// Override resolution walks the full linked class hierarchy (this jar and
    /// linked library/mod jars) considering only non-static, non-private
    /// instance overrides. Reaching vanilla `Item` or `BlockItem` proves there
    /// is no jar override (`Item.appendHoverText` is `0: return` and
    /// `BlockItem` does not override it in 26.3); an unresolvable superclass is
    /// counted rather than assumed to be a no-op.
    pub fn run_item_tooltip(
        &self,
        item: &Value,
        stack: Value,
        context: Value,
        display: Value,
        flag: Value,
        host: &mut StdHost,
        budget: &ExecutionBudget,
    ) -> Result<Vec<Value>, NativeError> {
        const DESCRIPTOR: &str = "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/Item$TooltipContext;Lnet/minecraft/world/item/component/TooltipDisplay;Ljava/util/function/Consumer;Lnet/minecraft/world/item/TooltipFlag;)V";
        const ACC_PRIVATE: u16 = 0x0002;
        let mut current = crate::jar_object::value_marker_class(item)
            .ok_or_else(|| NativeError::MissingMethod("Item#appendHoverText receiver".into()))?
            .to_owned();
        let linked = crate::Host::linked_class_table(host)
            .cloned()
            .unwrap_or_default();
        let mut override_owner = None;
        let mut reached_known_noop = false;
        for _ in 0..crate::MAX_SUPERCLASS_WALK {
            if matches!(
                current.as_str(),
                "net/minecraft/world/item/Item" | "net/minecraft/world/item/BlockItem"
            ) {
                reached_known_noop = true;
                break;
            }
            let class = self.classes.get(&current).or_else(|| {
                if crate::Host::models_class(host, &current) {
                    None
                } else {
                    linked.get(&current)
                }
            });
            let Some(class) = class else {
                break;
            };
            if let Some(method) = class.method("appendHoverText", DESCRIPTOR)
                && !method.is_static()
                && (method.access & ACC_PRIVATE) == 0
            {
                override_owner = Some(current.clone());
                break;
            }
            if class.super_name.is_empty() || class.super_name == current {
                break;
            }
            current = class.super_name.clone();
        }
        let Some(owner) = override_owner else {
            if !reached_known_noop {
                let _ = host.unknown_host_call(&MethodRef {
                    owner: current,
                    name: "appendHoverText".into(),
                    descriptor: DESCRIPTOR.into(),
                })?;
            }
            return Ok(Vec::new());
        };
        let sink = host.tooltip_sink();
        let args = [stack, context, display, sink.clone(), flag];
        let result = self.invoke_persistent(
            &MethodRef {
                owner,
                name: "appendHoverText".into(),
                descriptor: DESCRIPTOR.into(),
            },
            Some(item.clone()),
            &args,
            host,
            budget,
        );
        let lines = host.take_tooltip_sink(&sink);
        result.map(|_| lines)
    }

    /// Run one HUD element for this frame, through `host` (the jar's own,
    /// inside [`StdHost::begin_client_frame`]): its callback gets the
    /// graphics and delta tracker stand-ins. The host's call log is rolled
    /// back afterwards; the draws stay in the frame. An error (an unmodelled
    /// member, the budget, a refused bytecode) is the caller's to disable the
    /// element with.
    pub fn run_hud_layer(
        &self,
        layer: &HudLayer,
        host: &mut StdHost,
        budget: &ExecutionBudget,
    ) -> Result<(), NativeError> {
        let marks = host.call_log_marks();
        host.client.poses.clear();
        let args = vec![marker(GRAPHICS), marker(DELTA)];
        let result = self.invoke_listener_callback(&layer.callback, args, host, budget);
        host.truncate_call_log(marks);
        result.map(|_| ())
    }

    /// Run one `Runnable` a callback queued with `Minecraft.execute`, through
    /// `host` inside a client frame: a lambda of this jar, or an object of a
    /// jar class with a `run()` method. Anything else (a JDK or game
    /// runnable) is not a jar callback and refuses.
    pub fn run_client_task(
        &self,
        task: &Value,
        host: &mut StdHost,
        budget: &ExecutionBudget,
    ) -> Result<(), NativeError> {
        let callback = match crate::callback_marker::lambda_of(task) {
            Some(marker) => Some(ListenerCallback::Indy {
                target: marker.target,
                captures: marker.captures,
            }),
            None => events::jar_object(self, task).and_then(|(class, value)| {
                events::resolve_method(self, class, "run", "()V").map(|method| {
                    ListenerCallback::Object {
                        receiver: value.clone(),
                        method,
                    }
                })
            }),
        };
        let Some(callback) = callback else {
            return Err(NativeError::Metadata(
                "Minecraft.execute: the task is not a jar callback".into(),
            ));
        };
        let marks = host.call_log_marks();
        let result = self.invoke_listener_callback(&callback, Vec::new(), host, budget);
        host.truncate_call_log(marks);
        result.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(owner: &str, name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    fn snapshot() -> Arc<ClientSnapshot> {
        Arc::new(ClientSnapshot {
            gui_size: [320, 240],
            metrics: Arc::new(TextMetrics {
                advances: vec![6.0; 128],
                translations: BTreeMap::from([("hud.example.title".into(), "Title".into())]),
                answers: Default::default(),
            }),
            ..ClientSnapshot::default()
        })
    }

    #[test]
    fn item_tooltip_consumer_preserves_components() {
        let mut host = StdHost::new();
        let sink = host.tooltip_sink();
        let line = component_value("translatable", "demo.tooltip");
        crate::Host::invoke_instance(
            &mut host,
            &method(
                "java/util/function/Consumer",
                "accept",
                "(Ljava/lang/Object;)V",
            ),
            &sink,
            std::slice::from_ref(&line),
        )
        .unwrap();
        assert_eq!(host.take_tooltip_sink(&sink), vec![line]);
    }

    /// The inventory's lists are host collections (#156 P14d2): a call
    /// through `NonNullList` answers as the `List` member.
    #[test]
    fn a_non_null_list_call_on_an_inventory_list_answers_as_a_list() {
        let mut host = StdHost::new();
        host.begin_client_frame(snapshot(), &[]);
        let list = host.cached_collection(
            &marker(items::INVENTORY),
            "items",
            NON_NULL_LIST,
            true,
            vec![Value::Int(1), Value::Int(2)],
        );
        let size = method(NON_NULL_LIST, "size", "()I");
        assert_eq!(
            crate::Host::invoke_instance(&mut host, &size, &list, &[]).unwrap(),
            Value::Int(2)
        );
        assert!(host.unknown_host_calls().is_empty());
        // A mutator follows `NonNullList`'s own rules (no `null`, a fixed
        // size): counted.
        let add = method(NON_NULL_LIST, "add", "(Ljava/lang/Object;)Z");
        crate::Host::invoke_instance(&mut host, &add, &list, &[Value::Int(3)]).unwrap();
        assert_eq!(host.unknown_host_calls().len(), 1);
    }

    #[test]
    fn key_codes_name_their_input_constants() {
        // InputConstants.KEY_C, KEY_LSHIFT, KEY_F1 in 26.3 (SDL scancodes).
        assert_eq!(input_key_name(false, 6), "key.keyboard.c");
        assert_eq!(input_key_name(false, 225), "key.keyboard.left.shift");
        assert_eq!(input_key_name(false, 58), "key.keyboard.f1");
        assert_eq!(input_key_name(false, 0), "key.keyboard.unknown");
        assert_eq!(input_key_name(true, 2), "key.mouse.middle");
        assert_eq!(input_key_name(true, 4), "key.mouse.4");
    }

    /// `ARGB.white(float)` against Java 25.0.5 on the 26.3 jar (the `white` rows of
    /// `tests/fixtures/client_font/edge25.txt`): in range, the saturating `Mth.floor` far below it, a
    /// channel past 255 shifting its low byte, and NaN.
    #[test]
    fn argb_white_floors_as_a_double_and_saturates() {
        for (alpha, java) in [
            (0.5, 0x7fff_ffff),
            (1.0, 0xffff_ffff),
            (0.0, 0x00ff_ffff),
            (-1e10, 0x00ff_ffff),
            (f32::NEG_INFINITY, 0x00ff_ffff),
            (-9e6, 0x00ff_ffff),
            (2.0, 0xfeff_ffff),
            (f32::NAN, 0x00ff_ffff),
        ] {
            assert_eq!(argb_white(alpha), java, "{alpha}");
        }
    }

    /// The façade's draw calls land as ops in GUI pixels: the pose moves and
    /// scales them, a colour without alpha draws nothing, `centeredText`
    /// centres on the measured width.
    #[test]
    fn graphics_calls_record_draw_ops_through_the_pose() {
        let mut host = StdHost::new();
        host.begin_client_frame(snapshot(), &[]);
        let graphics = marker(GRAPHICS);
        let text = method(
            "net/minecraft/client/gui/GuiGraphicsExtractor",
            "text",
            "(Lnet/minecraft/client/gui/Font;Ljava/lang/String;IIIZ)V",
        );
        let font = marker(FONT);
        let call = |host: &mut StdHost, r: &MethodRef, receiver: &Value, args: &[Value]| {
            host.client_call(r, receiver, args).unwrap().unwrap()
        };
        call(
            &mut host,
            &text,
            &graphics,
            &[
                font.clone(),
                marker("hi"),
                Value::Int(2),
                Value::Int(3),
                Value::Int(-1),
                Value::Int(0),
            ],
        );
        // Alpha zero: vanilla draws nothing.
        call(
            &mut host,
            &text,
            &graphics,
            &[
                font.clone(),
                marker("gone"),
                Value::Int(0),
                Value::Int(0),
                Value::Int(0xFFFFFF),
                Value::Int(1),
            ],
        );
        let pose = call(
            &mut host,
            &method(
                "net/minecraft/client/gui/GuiGraphicsExtractor",
                "pose",
                "()Lorg/joml/Matrix3x2fStack;",
            ),
            &graphics,
            &[],
        );
        call(
            &mut host,
            &method(
                "org/joml/Matrix3x2fStack",
                "pushMatrix",
                "()Lorg/joml/Matrix3x2fStack;",
            ),
            &pose,
            &[],
        );
        call(
            &mut host,
            &method(
                "org/joml/Matrix3x2fStack",
                "translate",
                "(FF)Lorg/joml/Matrix3x2f;",
            ),
            &pose,
            &[Value::Float(10.0), Value::Float(20.0)],
        );
        call(
            &mut host,
            &method(
                "org/joml/Matrix3x2fStack",
                "scale",
                "(F)Lorg/joml/Matrix3x2f;",
            ),
            &pose,
            &[Value::Float(2.0)],
        );
        call(
            &mut host,
            &method(
                "net/minecraft/client/gui/GuiGraphicsExtractor",
                "fill",
                "(IIIII)V",
            ),
            &graphics,
            &[
                Value::Int(5),
                Value::Int(4),
                Value::Int(1),
                Value::Int(1),
                Value::Int(0x8000_0000u32 as i32),
            ],
        );
        call(
            &mut host,
            &method(
                "net/minecraft/client/gui/GuiGraphicsExtractor",
                "centeredText",
                "(Lnet/minecraft/client/gui/Font;Lnet/minecraft/network/chat/Component;III)V",
            ),
            &graphics,
            &[
                font.clone(),
                component_value("translatable", "hud.example.title"),
                Value::Int(0),
                Value::Int(0),
                Value::Int(-1),
            ],
        );
        call(
            &mut host,
            &method(
                "org/joml/Matrix3x2fStack",
                "popMatrix",
                "()Lorg/joml/Matrix3x2fStack;",
            ),
            &pose,
            &[],
        );
        let width = call(
            &mut host,
            &method(
                "net/minecraft/client/gui/Font",
                "width",
                "(Ljava/lang/String;)I",
            ),
            &font,
            &[marker("abc")],
        );
        assert_eq!(width, Value::Int(18));
        let out = host.end_client_frame();
        assert_eq!(
            out.draws,
            vec![
                DrawOp::Text {
                    text: ClientText::Literal("hi".into()),
                    at: [2.0, 3.0],
                    argb: 0xFFFF_FFFF,
                    shadow: false,
                    scale: [1.0, 1.0],
                },
                // (1,1)-(5,4) scaled by 2 and moved by (10, 20).
                DrawOp::Fill {
                    rect: [12.0, 22.0, 8.0, 6.0],
                    argb: 0x8000_0000,
                },
                // "Title" is 30 wide: x = 0 - 15, scaled by 2, then moved.
                DrawOp::Text {
                    text: ClientText::Translatable("hud.example.title".into()),
                    at: [-20.0, 20.0],
                    argb: 0xFFFF_FFFF,
                    shadow: true,
                    scale: [2.0, 2.0],
                },
            ]
        );
    }

    /// The player's equipment, inventory and effects answer from the frame:
    /// a damaged sword in the selected slot reads its durability, an armour
    /// slot's item is the same marker `Items.<NAME>` reads, a built stack
    /// starts from the item's facts, and an unlisted member still refuses.
    #[test]
    fn stacks_and_effects_answer_from_the_frame() {
        let sword = ClientStack {
            item: "minecraft:diamond_sword".into(),
            count: 1,
            damage: 400,
            max_damage: 1561,
            max_stack: 1,
            name: "Diamond Sword".into(),
            ..ClientStack::empty()
        };
        let helmet = ClientStack {
            item: "minecraft:iron_helmet".into(),
            count: 1,
            max_damage: 165,
            max_stack: 1,
            ..ClientStack::empty()
        };
        let mut inventory = vec![ClientStack::empty(); 36];
        inventory[2] = sword;
        let frame = Arc::new(ClientSnapshot {
            player: Some(ClientPlayer {
                inventory,
                selected: 2,
                armor: [
                    ClientStack::empty(),
                    ClientStack::empty(),
                    ClientStack::empty(),
                    helmet,
                ],
                effects: vec![ClientEffect {
                    id: "minecraft:speed".into(),
                    amplifier: 1,
                    duration: 600,
                    ambient: false,
                    visible: true,
                    show_icon: true,
                    category: "BENEFICIAL".into(),
                    color: 0x33ebff,
                }],
                ..ClientPlayer::default()
            }),
            items: Arc::new(ItemTable::from([(
                "minecraft:chest".into(),
                ClientItem {
                    max_stack: 64,
                    max_damage: 0,
                    name: "Chest".into(),
                    block: true,
                },
            )])),
            ..(*snapshot()).clone()
        });
        let mut host = StdHost::new();
        host.begin_client_frame(frame, &[]);
        let player = marker(PLAYER);
        let call = |host: &mut StdHost,
                    owner: &str,
                    name: &str,
                    desc: &str,
                    receiver: &Value,
                    args: &[Value]| {
            host.client_call(&method(owner, name, desc), receiver, args)
                .unwrap()
                .unwrap()
        };
        const PLAYER_CLASS: &str = "net/minecraft/world/entity/player/Player";
        const STACK_CLASS: &str = "net/minecraft/world/item/ItemStack";
        let held = call(
            &mut host,
            PLAYER_CLASS,
            "getMainHandItem",
            "()Lnet/minecraft/world/item/ItemStack;",
            &player,
            &[],
        );
        assert_eq!(
            call(&mut host, STACK_CLASS, "getMaxDamage", "()I", &held, &[]),
            Value::Int(1561)
        );
        assert_eq!(
            call(&mut host, STACK_CLASS, "getDamageValue", "()I", &held, &[]),
            Value::Int(400)
        );
        assert_eq!(
            call(&mut host, STACK_CLASS, "isDamaged", "()Z", &held, &[]),
            Value::Int(1)
        );
        // Item.getBarWidth: round(13 - 400 * 13 / 1561) = 10.
        assert_eq!(
            call(&mut host, STACK_CLASS, "getBarWidth", "()I", &held, &[]),
            Value::Int(10)
        );
        let head = call(
            &mut host,
            PLAYER_CLASS,
            "getItemBySlot",
            "(Lnet/minecraft/world/entity/EquipmentSlot;)Lnet/minecraft/world/item/ItemStack;",
            &player,
            &[marker("enum/net/minecraft/world/entity/EquipmentSlot/HEAD")],
        );
        let item = call(
            &mut host,
            STACK_CLASS,
            "getItem",
            "()Lnet/minecraft/world/item/Item;",
            &head,
            &[],
        );
        let constant = host
            .client_static(&method(
                "net/minecraft/world/item/Items",
                "IRON_HELMET",
                "Lnet/minecraft/world/item/Item;",
            ))
            .unwrap()
            .unwrap();
        assert_eq!(item, constant);
        // The feet slot is empty: vanilla's empty stack counts 0.
        let feet = call(
            &mut host,
            PLAYER_CLASS,
            "getItemBySlot",
            "(Lnet/minecraft/world/entity/EquipmentSlot;)Lnet/minecraft/world/item/ItemStack;",
            &player,
            &[marker("enum/net/minecraft/world/entity/EquipmentSlot/FEET")],
        );
        assert_eq!(
            call(&mut host, STACK_CLASS, "isEmpty", "()Z", &feet, &[]),
            Value::Int(1)
        );
        assert_eq!(
            call(&mut host, STACK_CLASS, "getCount", "()I", &feet, &[]),
            Value::Int(0)
        );
        // Inventory.getItem(39) is the head slot.
        let inventory = call(
            &mut host,
            PLAYER_CLASS,
            "getInventory",
            "()Lnet/minecraft/world/entity/player/Inventory;",
            &player,
            &[],
        );
        let slot39 = call(
            &mut host,
            "net/minecraft/world/entity/player/Inventory",
            "getItem",
            "(I)Lnet/minecraft/world/item/ItemStack;",
            &inventory,
            &[Value::Int(39)],
        );
        assert_eq!(slot39, head);
        // A built stack starts from the item's facts.
        let chest = host
            .client_static(&method(
                "net/minecraft/world/level/block/Blocks",
                "CHEST",
                "Lnet/minecraft/world/level/block/Block;",
            ))
            .unwrap()
            .unwrap();
        let built = marker("net/minecraft/world/item/ItemStack#7");
        call(
            &mut host,
            STACK_CLASS,
            "<init>",
            "(Lnet/minecraft/world/level/ItemLike;)V",
            &built,
            &[chest],
        );
        assert_eq!(
            call(
                &mut host,
                STACK_CLASS,
                "getMaxStackSize",
                "()I",
                &built,
                &[]
            ),
            Value::Int(64)
        );
        assert_eq!(
            call(&mut host, STACK_CLASS, "getCount", "()I", &built, &[]),
            Value::Int(1)
        );
        // Effects: the active one reads its duration through its holder.
        let speed = host
            .client_static(&method(
                "net/minecraft/world/effect/MobEffects",
                "SPEED",
                "Lnet/minecraft/core/Holder;",
            ))
            .unwrap()
            .unwrap();
        let effect = call(
            &mut host,
            PLAYER_CLASS,
            "getEffect",
            "(Lnet/minecraft/core/Holder;)Lnet/minecraft/world/effect/MobEffectInstance;",
            &player,
            &[speed],
        );
        assert_eq!(
            call(
                &mut host,
                "net/minecraft/world/effect/MobEffectInstance",
                "getDuration",
                "()I",
                &effect,
                &[]
            ),
            Value::Int(600)
        );
        // An unlisted stack member refuses.
        assert!(matches!(
            host.client_call(
                &method(STACK_CLASS, "getTooltipLines", "()Ljava/util/List;"),
                &held,
                &[]
            ),
            Some(Err(NativeError::UnknownHostCall(_)))
        ));
    }

    /// `java.awt.Color.GREEN.getRGB()` is opaque green, as on the JVM, through
    /// the host's own static and instance paths.
    #[test]
    fn awt_colour_constants_answer_their_argb() {
        let mut host = StdHost::new();
        let green = crate::Host::get_static(
            &mut host,
            &method("java/awt/Color", "GREEN", "Ljava/awt/Color;"),
        )
        .unwrap();
        let rgb = crate::Host::invoke_instance(
            &mut host,
            &method("java/awt/Color", "getRGB", "()I"),
            &green,
            &[],
        )
        .unwrap();
        assert_eq!(rgb, Value::Int(0xff00_ff00_u32 as i32));
        let upper = crate::Host::get_static(
            &mut host,
            &method("java/awt/Color", "LIGHT_GRAY", "Ljava/awt/Color;"),
        )
        .unwrap();
        let lower = crate::Host::get_static(
            &mut host,
            &method("java/awt/Color", "lightGray", "Ljava/awt/Color;"),
        )
        .unwrap();
        assert_eq!(upper, lower, "upper={upper:?} lower={lower:?}");
    }

    #[test]
    fn an_unmodelled_member_refuses_only_inside_a_firing() {
        let mut host = StdHost::new();
        let r = method(
            "net/minecraft/client/Minecraft",
            "setScreen",
            "(Lnet/minecraft/client/gui/screens/Screen;)V",
        );
        // Bootstrap: the host's usual lenient path answers.
        assert!(
            host.client_call(&r, &marker(MINECRAFT), &[Value::Ref(None)])
                .is_none()
        );
        host.begin_client_frame(snapshot(), &[]);
        assert!(matches!(
            host.client_call(&r, &marker(MINECRAFT), &[Value::Ref(None)]),
            Some(Err(NativeError::UnknownHostCall(_)))
        ));
        let rotate = method(
            "org/joml/Matrix3x2fStack",
            "rotate",
            "(F)Lorg/joml/Matrix3x2f;",
        );
        assert!(matches!(
            host.client_call(&rotate, &marker(POSE), &[Value::Float(1.0)]),
            Some(Err(NativeError::UnknownHostCall(_)))
        ));
    }

    /// A container screen's frame (`screen_scope`) fills only the GUI size and the presence of
    /// the player and level: the graphics surface, its pose, `Entity.level()` and the inventory's
    /// constant name answer; the player's facts, its inventory, `Minecraft.screen`, the options
    /// and font measurement are refused and counted, even when the snapshot happens to hold
    /// values for them.
    #[test]
    fn a_screen_frame_answers_only_its_allow_list() {
        let mut host = StdHost::new();
        host.begin_client_frame(
            Arc::new(ClientSnapshot {
                player: Some(ClientPlayer {
                    health: 20.0,
                    xp_level: 7,
                    inventory: vec![ClientStack::empty(); 36],
                    ..ClientPlayer::default()
                }),
                level: Some(ClientLevel::default()),
                gui_size: [320, 240],
                screen_scope: true,
                ..ClientSnapshot::default()
            }),
            &[],
        );
        let graphics = "net/minecraft/client/gui/GuiGraphicsExtractor";
        assert!(matches!(
            host.client_call(&method(graphics, "guiWidth", "()I"), &marker(GRAPHICS), &[]),
            Some(Ok(Value::Int(320)))
        ));
        let translate = method(
            "org/joml/Matrix3x2fStack",
            "translate",
            "(FF)Lorg/joml/Matrix3x2f;",
        );
        assert!(matches!(
            host.client_call(
                &translate,
                &marker(POSE),
                &[Value::Float(2.0), Value::Float(3.0)]
            ),
            Some(Ok(_))
        ));
        let fill = method(graphics, "fill", "(IIIII)V");
        let corners = [1, 2, 5, 6, 0xFF00_00FFu32 as i32].map(Value::Int);
        assert!(matches!(
            host.client_call(&fill, &marker(GRAPHICS), &corners),
            Some(Ok(Value::Void))
        ));
        assert_eq!(
            host.client_draws(),
            &[DrawOp::Fill {
                rect: [3.0, 5.0, 4.0, 4.0],
                argb: 0xFF00_00FF,
            }]
        );
        let level = method(
            "net/minecraft/world/entity/Entity",
            "level",
            "()Lnet/minecraft/world/level/Level;",
        );
        assert!(matches!(
            host.client_call(&level, &marker(PLAYER), &[]),
            Some(Ok(value)) if value == marker(LEVEL)
        ));
        let name = method(
            "net/minecraft/world/entity/player/Inventory",
            "getDisplayName",
            "()Lnet/minecraft/network/chat/Component;",
        );
        assert!(matches!(
            host.client_call(&name, &marker(items::INVENTORY), &[]),
            Some(Ok(value)) if value == crate::host_component::inventory_default_name()
        ));
        let before = host.unknown_host_call_count();
        let refused = [
            (
                method(
                    "net/minecraft/world/entity/LivingEntity",
                    "getHealth",
                    "()F",
                ),
                PLAYER,
            ),
            (
                method("net/minecraft/world/level/Level", "isClientSide", "()Z"),
                LEVEL,
            ),
            (
                method(
                    "net/minecraft/world/entity/player/Inventory",
                    "getItem",
                    "(I)Lnet/minecraft/world/item/ItemStack;",
                ),
                items::INVENTORY,
            ),
            (
                method(
                    "net/minecraft/client/DeltaTracker",
                    "getGameTimeDeltaPartialTick",
                    "(Z)F",
                ),
                DELTA,
            ),
        ];
        for (r, receiver) in &refused {
            // `getItem(0)` and `getGameTimeDeltaPartialTick(false)` take one int argument.
            let args = if r.descriptor.starts_with("(I)") || r.descriptor.starts_with("(Z)") {
                vec![Value::Int(0)]
            } else {
                Vec::new()
            };
            assert!(
                matches!(
                    host.client_call(r, &marker(*receiver), &args),
                    Some(Err(NativeError::UnknownHostCall(_)))
                ),
                "{r:?}"
            );
        }
        assert_eq!(host.unknown_host_call_count(), before + refused.len());
        let centered = method(
            graphics,
            "centeredText",
            "(Lnet/minecraft/client/gui/Font;Ljava/lang/String;III)V",
        );
        let centered_args = [
            marker(FONT),
            Value::Ref(Some("Title".into())),
            Value::Int(10),
            Value::Int(10),
            Value::Int(-1),
        ];
        assert!(matches!(
            host.client_call(&centered, &marker(GRAPHICS), &centered_args),
            Some(Err(NativeError::UnknownHostCall(_)))
        ));
        assert!(host.client_inventory_stack(0).is_none());
        for (field, descriptor) in [
            ("screen", "Lnet/minecraft/client/gui/screens/Screen;"),
            ("player", "Lnet/minecraft/client/player/LocalPlayer;"),
        ] {
            let r = method(MINECRAFT_CLASS, field, descriptor);
            let answer = host.client_static(&r);
            match field {
                "screen" => assert!(matches!(answer, Some(Err(_)))),
                _ => assert!(matches!(answer, Some(Ok(value)) if value == marker(PLAYER))),
            }
        }
        let xp = method(
            "net/minecraft/world/entity/player/Player",
            "experienceLevel",
            "I",
        );
        assert!(matches!(host.client_static(&xp), Some(Err(_))));
        let hide = method(OPTIONS_CLASS, "hideGui", "Z");
        assert!(matches!(host.client_static(&hide), Some(Err(_))));
    }

    #[test]
    fn a_server_host_has_no_client_surface() {
        let get_instance = method(
            MINECRAFT_CLASS,
            "getInstance",
            "()Lnet/minecraft/client/Minecraft;",
        );
        let mut client = StdHost::new();
        assert_eq!(
            client.client_invoke(&get_instance, &[]).unwrap().unwrap(),
            marker(MINECRAFT)
        );
        // A server never has a `Minecraft`: the call keeps the host's old answer.
        let mut server = StdHost::new().with_environment_type("SERVER");
        assert!(server.client_invoke(&get_instance, &[]).is_none());
    }

    #[test]
    fn a_registered_mapping_answers_clicks_from_its_key_state() {
        let mut host = StdHost::new();
        let object = Value::Host(crate::HostRef::new(
            crate::HostKind::JarObject,
            "net/minecraft/client/KeyMapping#0",
        ));
        let category = host
            .client_static(&method(
                KEY_CATEGORY,
                "MISC",
                "Lnet/minecraft/client/KeyMapping$Category;",
            ))
            .unwrap()
            .unwrap();
        host.client_call(
            &method(
                KEY_MAPPING,
                "<init>",
                "(Ljava/lang/String;ILnet/minecraft/client/KeyMapping$Category;)V",
            ),
            &object,
            &[marker("key.example.zoom"), Value::Int(6), category],
        );
        let registered = host
            .client_invoke(
                &method(
                    KEY_HELPER,
                    "registerKeyMapping",
                    "(Lnet/minecraft/client/KeyMapping;)Lnet/minecraft/client/KeyMapping;",
                ),
                std::slice::from_ref(&object),
            )
            .unwrap()
            .unwrap();
        // Fabric hands the same mapping back for the mod to keep.
        assert_eq!(registered, object);
        assert_eq!(
            host.client_key_mappings(),
            &[ClientKeyMapping {
                marker: "net/minecraft/client/KeyMapping#0".into(),
                name: "key.example.zoom".into(),
                default_key: "key.keyboard.c".into(),
                category: "minecraft:misc".into(),
                source: "KeyMappingHelper.registerKeyMapping",
            }]
        );
        let consume = method(KEY_MAPPING, "consumeClick", "()Z");
        host.begin_client_frame(
            snapshot(),
            &[KeyPress {
                down: true,
                clicks: 2,
            }],
        );
        let mut clicks = 0;
        while host.client_call(&consume, &object, &[]).unwrap().unwrap() == Value::Int(1) {
            clicks += 1;
        }
        assert_eq!(clicks, 2);
        let down = host.client_call(&method(KEY_MAPPING, "isDown", "()Z"), &object, &[]);
        assert_eq!(down.unwrap().unwrap(), Value::Int(1));
        assert_eq!(
            host.end_client_frame().keys,
            vec![KeyPress {
                down: true,
                clicks: 0
            }]
        );
    }

    #[test]
    fn graphics_text_refuses_unresolvable_component_instead_of_drawing_nothing() {
        let mut host = StdHost::new();
        let bad = crate::host_component::translatable("k");
        let bad_arg = crate::jar_object::handle("java/lang/Object#0".into());
        crate::host_component::change(
            &mut host.component_states,
            &bad,
            &crate::facade_text::argument,
            |state| state.args = Some(vec![bad_arg]),
        )
        .unwrap();
        host.begin_client_frame(snapshot(), &[]);
        let gfx = marker(GRAPHICS);
        let font = marker(FONT);
        for name in ["text", "centeredText"] {
            let str_overload = method(
                "net/minecraft/client/gui/GuiGraphicsExtractor",
                name,
                "(Lnet/minecraft/client/gui/Font;Ljava/lang/String;III)V",
            );
            let comp_overload = method(
                "net/minecraft/client/gui/GuiGraphicsExtractor",
                name,
                "(Lnet/minecraft/client/gui/Font;Lnet/minecraft/network/chat/Component;III)V",
            );
            // Null String draws nothing; null Component throws NPE; unresolvable Component refuses.
            assert_eq!(
                host.client_call(
                    &str_overload,
                    &gfx,
                    &[
                        font.clone(),
                        Value::Ref(None),
                        Value::Int(0),
                        Value::Int(0),
                        Value::Int(0),
                    ],
                )
                .unwrap()
                .unwrap(),
                Value::Void,
                "{name}(String null)"
            );
            assert!(
                matches!(
                    host.client_call(
                        &comp_overload,
                        &gfx,
                        &[
                            font.clone(),
                            Value::Ref(None),
                            Value::Int(0),
                            Value::Int(0),
                            Value::Int(0),
                        ],
                    ),
                    Some(Err(NativeError::UncaughtException { ref class, .. }))
                        if class == "java/lang/NullPointerException"
                ),
                "{name}(Component null)"
            );
            assert!(
                matches!(
                    host.client_call(
                        &comp_overload,
                        &gfx,
                        &[
                            font.clone(),
                            bad.clone(),
                            Value::Int(0),
                            Value::Int(0),
                            Value::Int(0),
                        ],
                    ),
                    Some(Err(NativeError::UnknownHostCall(_)))
                ),
                "{name}(unresolvable)"
            );
        }
        let dec = method(
            "net/minecraft/client/gui/GuiGraphicsExtractor",
            "itemDecorations",
            "(Lnet/minecraft/client/gui/Font;Lnet/minecraft/world/item/ItemStack;IILjava/lang/String;)V",
        );
        assert!(matches!(
            host.client_call(
                &dec,
                &gfx,
                &[
                    font,
                    marker(format!("{}main-hand", items::STACK)),
                    Value::Int(0),
                    Value::Int(0),
                    bad,
                ],
            ),
            Some(Err(NativeError::UnknownHostCall(_)))
        ));
        host.end_client_frame();
    }

    #[test]
    fn inventory_player_field_checks_descriptor_and_frame_player_presence() {
        use crate::Host;

        let inv = crate::game_object::client_value("client/inventory".into());
        let valid_field = method(
            "net/minecraft/world/entity/player/Inventory",
            "player",
            "Lnet/minecraft/world/entity/player/Player;",
        );
        let wrong_desc = method(
            "net/minecraft/world/entity/player/Inventory",
            "player",
            "Ljava/lang/Object;",
        );

        // No frame / no player -> counted.
        let mut host = StdHost::new();
        let _ = host.get_field(&inv, &valid_field);
        assert!(host.unknown_host_call_count() > 0);

        let mut host = StdHost::new();
        host.begin_client_frame(Arc::new(ClientSnapshot::default()), &[]);
        let _ = host.get_field(&inv, &valid_field);
        assert!(host.unknown_host_call_count() > 0);

        // Frame with player: exact descriptor answers client/player; wrong descriptor is counted.
        let mut host = StdHost::new();
        let with_player = Arc::new(ClientSnapshot {
            player: Some(ClientPlayer::default()),
            ..(*snapshot()).clone()
        });
        host.begin_client_frame(with_player, &[]);
        let got = host.get_field(&inv, &valid_field).unwrap();
        assert_eq!(crate::game_object::handle_key(&got), Some("client/player"));
        assert_eq!(host.unknown_host_call_count(), 0);
        let _ = host.get_field(&inv, &wrong_desc);
        assert!(host.unknown_host_call_count() > 0);
    }
}
