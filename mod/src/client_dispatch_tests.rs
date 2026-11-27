//! Contract and end-to-end tests for the Sunrise-facing client adapters:
//! - `Hooks::build_creative_mode_tab_contents` -> Fabric `CreativeModeTabEvents` listeners
//! - `Hooks::item_tooltip` -> Java `Item.appendHoverText` + Sunrise rich `Text` boundary

use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::__private::game::BackendRefOf;
use mod_api::text::{TextContent, TextStyle};
use mod_api::{
    Backend, BackendRef, BuildCreativeModeTabContentsEvent, ClientHost, ClientReloadHost,
    ClientSetupContext, ClientSetupEvent, CreativeTabChange, CreativeTabEdit, CreativeTabEditError,
    CreativeTabList, CreativeTabPermissions, CreativeTabPlacement, CreativeTabVisibility, Dist,
    HookId, Hooks, Host, HostSide, Identifier, ItemKey, ItemStack, ItemStackView, ItemTooltipEvent,
    Listener, ModEntry, ReloadContext, Tag, Text, TextFormError, TooltipDisplay, TooltipLines,
    TooltipLinesAccess,
};
use zip::write::SimpleFileOptions;

use crate::HorizonJava;

static FIXTURE_BUILDS: AtomicU64 = AtomicU64::new(0);

/// Build a Fabric jar from Java sources with `javac --release 21`, packing only `demo/` classes
/// plus `fabric.mod.json` so every `net/minecraft/*` and `net/fabricmc/*` call crosses the
/// native runtime boundary.
fn build_javac_jar(test: &str, mod_json: &str, sources: &[(&str, &str)]) -> Vec<u8> {
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
    let build = FIXTURE_BUILDS.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "horizon-java-mod-client-{test}-{}-{unique}-{build}",
        std::process::id()
    ));
    let _cleanup = Cleanup(root.clone());
    let classes = root.join("classes");

    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| PathBuf::from("javac"));
    let mut cmd = Command::new(javac);
    cmd.args(["--release", "21", "-d"]).arg(&classes);
    for (path, source) in sources {
        let file = root.join("src").join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        cmd.arg(file);
    }
    let output = cmd.output().expect("invoke javac");
    assert!(
        output.status.success(),
        "javac failed for {test}:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_files(&path, out);
            } else {
                out.push(path);
            }
        }
    }

    let mut class_files = Vec::new();
    collect_files(&classes.join("demo"), &mut class_files);
    class_files.sort();

    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        let opts = SimpleFileOptions::default();
        zip.start_file("fabric.mod.json", opts).unwrap();
        zip.write_all(mod_json.as_bytes()).unwrap();
        for class in class_files {
            let rel = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(rel, opts).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();
    }
    bytes.into_inner()
}

/// Forwarding mod entry that preserves `HorizonJava`'s exact `#[mod_api::sunrise_mod]` hook
/// subscriptions while allowing the test to inspect `HorizonJava`'s client state between calls.
struct SharedHorizonJava(Arc<HorizonJava>);

impl Hooks for SharedHorizonJava {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        self.0.subscribed()
    }
    fn listeners(&self) -> &'static [Listener] {
        self.0.listeners()
    }
    fn client_setup(&self, context: &mut dyn ClientSetupContext, event: &mut ClientSetupEvent) {
        self.0.client_setup(context, event);
    }
    fn build_creative_mode_tab_contents(
        &self,
        host: HostSide<'_>,
        event: &mut BuildCreativeModeTabContentsEvent<'_>,
    ) {
        self.0.build_creative_mode_tab_contents(host, event);
    }
    fn item_tooltip(&self, host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
        self.0.item_tooltip(host, event);
    }
}

#[derive(Default)]
struct CreativeTabEditorState {
    active: Option<CreativeTabEdit>,
    parent: Vec<ItemStack>,
    search: Vec<ItemStack>,
}

/// Sunrise client host test double implementing the real creative-tab editor (`ItemsBackend`),
/// item registry keys (`ItemsHost` / `KeysBackend`), `TextHost::text_from_json` (matching
/// Sunrise's `native_text_json` component codec), and `ConfigHost::game_version`.
struct TestClientHost {
    fake: mod_api::testing::FakeHost,
    /// While set, `KeysBackend::item_key_id` names no item, as a host with no name for a slot.
    hide_item_names: std::cell::Cell<bool>,
    editor: RefCell<CreativeTabEditorState>,
    /// Every `ClientHost::play_local_sound` the bridge made, with its `after` ticks.
    local_sounds: RefCell<Vec<(mod_api::SoundPlay, u32)>>,
}

impl TestClientHost {
    fn new(item_ids: &[&str]) -> Self {
        let mut fake = mod_api::testing::FakeHost::new();
        let parsed: Vec<Identifier> = item_ids
            .iter()
            .map(|id| Identifier::parse(id).unwrap_or_else(|e| panic!("bad id {id}: {e:?}")))
            .collect();
        fake.registries
            .resolve_items(parsed)
            .expect("resolve test items");
        Self {
            fake,
            hide_item_names: std::cell::Cell::new(false),
            editor: RefCell::new(CreativeTabEditorState::default()),
            local_sounds: RefCell::default(),
        }
    }

    fn stack(&self, id: &str) -> ItemStack {
        let key = mod_api::prelude::ItemsHost::item_key(self, &Identifier::parse(id).unwrap())
            .unwrap_or_else(|| panic!("unknown test item {id}"));
        ItemStack::of(key)
    }

    fn begin_tab(&self, id: u64, parent: &[&str], search: &[&str]) -> CreativeTabEdit {
        let edit = mod_api::__private::creative_tab_edit(id);
        let mut editor = self.editor.borrow_mut();
        editor.active = Some(edit);
        editor.parent = parent.iter().map(|s| self.stack(s)).collect();
        editor.search = search.iter().map(|s| self.stack(s)).collect();
        edit
    }

    fn tab_entries(&self, list: CreativeTabList) -> Vec<(String, u32)> {
        let editor = self.editor.borrow();
        let entries = match list {
            CreativeTabList::Parent => &editor.parent,
            CreativeTabList::Search => &editor.search,
        };
        entries
            .iter()
            .map(|stack| {
                let id = stack.item().id(self).expect("a test item has a name");
                (id.to_owned(), stack.count())
            })
            .collect()
    }
}

impl Host for TestClientHost {
    fn backend(&self) -> Option<BackendRef<'_>> {
        Some(BackendRef::of(self))
    }
}
mod_api::impl_host_areas!(TestClientHost, except(config, items, text));

impl ClientHost for TestClientHost {}
mod_api::impl_client_host_areas!(TestClientHost, except(effects));

impl mod_api::prelude::EffectsClientHost for TestClientHost {
    fn play_local_sound(
        &self,
        sound: &mod_api::SoundPlay,
        after: u32,
    ) -> Option<mod_api::SoundRef> {
        self.local_sounds.borrow_mut().push((sound.clone(), after));
        Some(mod_api::__private::game::sound_ref(
            self.local_sounds.borrow().len() as u64,
        ))
    }
}

/// `client_setup`'s context (since d404b5f02f it is not a `ClientHost`): the same test host,
/// with the version the game reports and the same local-sound recorder.
impl ReloadContext for TestClientHost {
    fn dist(&self) -> Dist {
        Dist::Client
    }
    fn game_version(&self) -> &'static str {
        "26.3"
    }
}
impl ClientReloadHost for TestClientHost {}
impl ClientSetupContext for TestClientHost {
    fn play_local_sound(
        &self,
        sound: &mod_api::SoundPlay,
        after: u32,
    ) -> Option<mod_api::SoundRef> {
        mod_api::prelude::EffectsClientHost::play_local_sound(self, sound, after)
    }
}

/// The engine's live tooltip line list over a plain vector, as `item_tooltip` lends it since
/// d404b5f02f (`TooltipLines`): each written line is validated, as the game's list does.
struct VecLines<'v>(&'v mut Vec<(Text, u32)>);

impl TooltipLinesAccess for VecLines<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn color(&self, index: usize) -> Option<u32> {
        self.0.get(index).map(|line| line.1)
    }
    fn get(&self, index: usize) -> Option<Result<(Text, u32), TextFormError>> {
        self.0.get(index).cloned().map(Ok)
    }
    fn push(&mut self, text: Text, color: u32) -> Result<(), TextFormError> {
        text.validate()?;
        self.0.push((text, color));
        Ok(())
    }
    fn insert(&mut self, index: usize, text: Text, color: u32) -> Result<bool, TextFormError> {
        if index > self.0.len() {
            return Ok(false);
        }
        text.validate()?;
        self.0.insert(index, (text, color));
        Ok(true)
    }
    fn set(&mut self, index: usize, text: Text, color: u32) -> Result<bool, TextFormError> {
        if index >= self.0.len() {
            return Ok(false);
        }
        text.validate()?;
        self.0[index] = (text, color);
        Ok(true)
    }
    fn set_color(&mut self, index: usize, color: u32) -> bool {
        match self.0.get_mut(index) {
            Some(line) => {
                line.1 = color;
                true
            }
            None => false,
        }
    }
    fn remove(&mut self, index: usize) -> Option<Result<(Text, u32), TextFormError>> {
        (index < self.0.len()).then(|| Ok(self.0.remove(index)))
    }
    fn pop(&mut self) -> Option<Result<(Text, u32), TextFormError>> {
        self.0.pop().map(Ok)
    }
    fn clear(&mut self) {
        self.0.clear();
    }
    fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }
    fn swap(&mut self, a: usize, b: usize) -> bool {
        if a < self.0.len() && b < self.0.len() {
            self.0.swap(a, b);
            true
        } else {
            false
        }
    }
}

impl Backend for TestClientHost {}
mod_api::impl_backend_areas!(TestClientHost, except(items, keys));

impl mod_api::prelude::ConfigHost for TestClientHost {
    fn dist(&self) -> Dist {
        Dist::Client
    }
    fn game_version(&self) -> &'static str {
        "26.3"
    }
    fn on(&self, _mod_id: &str) -> bool {
        true
    }
}

impl mod_api::prelude::ItemsHost for TestClientHost {
    fn item_key(&self, id: &Identifier) -> Option<ItemKey> {
        mod_api::prelude::ItemsHost::item_key(&self.fake, id)
    }
}

impl mod_api::prelude::KeysBackend for TestClientHost {
    fn item_key_id(&self, key: ItemKey) -> Option<&str> {
        if self.hide_item_names.get() {
            return None;
        }
        mod_api::prelude::KeysBackend::item_key_id(&self.fake, key)
    }
}

impl mod_api::prelude::ItemsBackend for TestClientHost {
    fn creative_tab_entries(
        &self,
        edit: CreativeTabEdit,
        list: CreativeTabList,
    ) -> Result<Vec<ItemStack>, CreativeTabEditError> {
        let editor = self.editor.borrow();
        if editor.active != Some(edit) {
            return Err(CreativeTabEditError::Stale);
        }
        Ok(match list {
            CreativeTabList::Parent => editor.parent.clone(),
            CreativeTabList::Search => editor.search.clone(),
        })
    }

    fn creative_tab_change(
        &self,
        edit: CreativeTabEdit,
        change: CreativeTabChange,
    ) -> Result<(), CreativeTabEditError> {
        let mut editor = self.editor.borrow_mut();
        if editor.active != Some(edit) {
            return Err(CreativeTabEditError::Stale);
        }
        match change {
            CreativeTabChange::Insert {
                placement,
                stack,
                visibility,
            } => {
                if stack.count() != 1 {
                    return Err(CreativeTabEditError::Count);
                }
                let lists: &[CreativeTabList] = match visibility {
                    CreativeTabVisibility::Parent => &[CreativeTabList::Parent],
                    CreativeTabVisibility::Search => &[CreativeTabList::Search],
                    CreativeTabVisibility::Both => {
                        &[CreativeTabList::Parent, CreativeTabList::Search]
                    }
                };
                // Validate every target list before mutating, matching Sunrise's editor.
                let mut indices = Vec::with_capacity(lists.len());
                for &list in lists {
                    let entries = match list {
                        CreativeTabList::Parent => &editor.parent,
                        CreativeTabList::Search => &editor.search,
                    };
                    if entries
                        .iter()
                        .any(|existing| existing.is_stackable_with(&stack))
                    {
                        return Err(CreativeTabEditError::Duplicate(list));
                    }
                    let index = match &placement {
                        CreativeTabPlacement::First => 0,
                        CreativeTabPlacement::Last => entries.len(),
                        CreativeTabPlacement::Before(anchor) => entries
                            .iter()
                            .position(|existing| existing.is_stackable_with(anchor))
                            .ok_or(CreativeTabEditError::MissingAnchor(list))?,
                        CreativeTabPlacement::After(anchor) => entries
                            .iter()
                            .rposition(|existing| existing.is_stackable_with(anchor))
                            .map(|idx| idx + 1)
                            .ok_or(CreativeTabEditError::MissingAnchor(list))?,
                    };
                    indices.push((list, index));
                }
                for (list, index) in indices {
                    let entries = match list {
                        CreativeTabList::Parent => &mut editor.parent,
                        CreativeTabList::Search => &mut editor.search,
                    };
                    entries.insert(index, stack.clone());
                }
                Ok(())
            }
            CreativeTabChange::Remove { stack, visibility } => {
                if matches!(
                    visibility,
                    CreativeTabVisibility::Parent | CreativeTabVisibility::Both
                ) {
                    editor
                        .parent
                        .retain(|existing| !existing.is_stackable_with(&stack));
                }
                if matches!(
                    visibility,
                    CreativeTabVisibility::Search | CreativeTabVisibility::Both
                ) {
                    editor
                        .search
                        .retain(|existing| !existing.is_stackable_with(&stack));
                }
                Ok(())
            }
        }
    }
}

impl mod_api::prelude::TextHost for TestClientHost {
    fn text_from_json(&self, json: &str) -> Result<Option<Text>, TextFormError> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            return Ok(None);
        };
        Ok(parse_json_component(&value, false))
    }
}

/// Parse vanilla `ComponentSerialization.CODEC` JSON into Sunrise [`Text`], matching Sunrise's
/// `native_text_json` rules (returns `None` on malformed or unsupported shapes).
fn parse_json_component(value: &serde_json::Value, allow_primitive: bool) -> Option<Text> {
    match value {
        serde_json::Value::String(s) => Some(Text::literal(s.clone())),
        serde_json::Value::Bool(b) if allow_primitive => Some(Text::literal(b.to_string())),
        serde_json::Value::Number(n) if allow_primitive => Some(Text::literal(n.to_string())),
        serde_json::Value::Array(items) => {
            let (first, rest) = items.split_first()?;
            let mut root = parse_json_component(first, false)?;
            for item in rest {
                root.children.push(parse_json_component(item, false)?);
            }
            Some(root)
        }
        serde_json::Value::Object(map) => {
            let content = if let Some(text) = map.get("text") {
                TextContent::Literal(text.as_str()?.to_owned())
            } else {
                let key = map.get("translate")?.as_str()?.to_owned();
                let fallback = match map.get("fallback") {
                    None => None,
                    Some(v) => Some(v.as_str()?.to_owned()),
                };
                let arguments = match map.get("with") {
                    None => Vec::new(),
                    Some(serde_json::Value::Array(args)) => args
                        .iter()
                        .map(|arg| parse_json_component(arg, true))
                        .collect::<Option<Vec<_>>>()?,
                    Some(_) => return None,
                };
                TextContent::Translatable {
                    key,
                    fallback,
                    arguments,
                }
            };
            let mut style = TextStyle::default();
            if let Some(color) = map.get("color") {
                style.color = Some(color.as_str()?.to_owned());
            }
            if let Some(bold) = map.get("bold") {
                style.bold = Some(bold.as_bool()?);
            }
            if let Some(italic) = map.get("italic") {
                style.italic = Some(italic.as_bool()?);
            }
            if let Some(underlined) = map.get("underlined") {
                style.underlined = Some(underlined.as_bool()?);
            }
            if let Some(strikethrough) = map.get("strikethrough") {
                style.strikethrough = Some(strikethrough.as_bool()?);
            }
            if let Some(obfuscated) = map.get("obfuscated") {
                style.obfuscated = Some(obfuscated.as_bool()?);
            }
            let mut text = Text::new(content).with_style(style);
            if let Some(extra) = map.get("extra") {
                let items = extra.as_array()?;
                if items.is_empty() {
                    return None;
                }
                for item in items {
                    text.children.push(parse_json_component(item, false)?);
                }
            }
            Some(text)
        }
        _ => None,
    }
}

/// Compile-only Minecraft 26.3 + Fabric API 5.0.21 stubs for the client dispatch fixture.
const CLIENT_DISPATCH_STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api; public interface ModInitializer { void onInitialize(); }",
    ),
    (
        "net/minecraft/ChatFormatting.java",
        "package net.minecraft; public enum ChatFormatting { BLACK, DARK_BLUE, DARK_GREEN, DARK_AQUA, DARK_RED, DARK_PURPLE, GOLD, GRAY, DARK_GRAY, BLUE, GREEN, AQUA, RED, LIGHT_PURPLE, YELLOW, WHITE, OBFUSCATED, BOLD, STRIKETHROUGH, UNDERLINE, ITALIC, RESET; }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String ns, String path) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources; public final class ResourceKey<T> { public static <T> ResourceKey<T> create(ResourceKey<?> reg, Identifier id) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core; import net.minecraft.resources.Identifier; public interface Registry<T> { static <V, T extends V> T register(Registry<V> registry, Identifier id, T value) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        "package net.minecraft.core; public interface DefaultedRegistry<T> extends Registry<T> {}",
    ),
    (
        "net/minecraft/core/component/DataComponentType.java",
        "package net.minecraft.core.component; public interface DataComponentType<T> {}",
    ),
    (
        "net/minecraft/core/component/DataComponents.java",
        r#"package net.minecraft.core.component;
import net.minecraft.network.chat.Component;
import net.minecraft.util.Unit;
import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.item.component.TooltipDisplay;
public final class DataComponents {
    public static final DataComponentType<Integer> DAMAGE = null;
    public static final DataComponentType<Integer> MAX_DAMAGE = null;
    public static final DataComponentType<Component> CUSTOM_NAME = null;
    public static final DataComponentType<CustomData> CUSTOM_DATA = null;
    public static final DataComponentType<Unit> UNBREAKABLE = null;
    public static final DataComponentType<Object> LORE = null;
    public static final DataComponentType<Object> BANNER_PATTERNS = null;
    public static final DataComponentType<Object> ENCHANTMENTS = null;
    public static final DataComponentType<TooltipDisplay> TOOLTIP_DISPLAY = null;
}"#,
    ),
    (
        "net/minecraft/util/Unit.java",
        "package net.minecraft.util; public enum Unit { INSTANCE }",
    ),
    (
        "net/minecraft/nbt/CompoundTag.java",
        "package net.minecraft.nbt; public class CompoundTag { public CompoundTag() {} public void putInt(String k, int v) {} public int getIntOr(String k, int def) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/world/item/component/CustomData.java",
        "package net.minecraft.world.item.component; import net.minecraft.core.component.DataComponentType; import net.minecraft.nbt.CompoundTag; import net.minecraft.world.item.ItemStack; public final class CustomData { public static void set(DataComponentType<CustomData> type, ItemStack stack, CompoundTag tag) {} public CompoundTag copyTag() { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.DefaultedRegistry; import net.minecraft.core.Registry; import net.minecraft.core.component.DataComponentType; import net.minecraft.world.item.Item; public final class BuiltInRegistries { public static final DefaultedRegistry<Item> ITEM = null; public static final Registry<DataComponentType<?>> DATA_COMPONENT_TYPE = null; }",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat; public interface Component { static MutableComponent translatable(String k) { throw new AssertionError(); } static MutableComponent translatable(String k, Object... args) { throw new AssertionError(); } static MutableComponent literal(String s) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat; import net.minecraft.ChatFormatting; public interface MutableComponent extends Component { MutableComponent withStyle(ChatFormatting... styles); }",
    ),
    (
        "net/minecraft/world/flag/FeatureFlag.java",
        "package net.minecraft.world.flag; public class FeatureFlag {}",
    ),
    (
        "net/minecraft/world/flag/FeatureFlagSet.java",
        "package net.minecraft.world.flag; public final class FeatureFlagSet {}",
    ),
    (
        "net/minecraft/world/flag/FeatureFlags.java",
        "package net.minecraft.world.flag; public final class FeatureFlags { public static final FeatureFlag VANILLA = null; public static final FeatureFlag TRADE_REBALANCE = null; }",
    ),
    (
        "net/minecraft/world/level/ItemLike.java",
        "package net.minecraft.world.level; import net.minecraft.world.item.Item; public interface ItemLike { Item asItem(); }",
    ),
    (
        "net/minecraft/world/item/component/TooltipDisplay.java",
        r#"package net.minecraft.world.item.component;
import net.minecraft.core.component.DataComponentType;
public final class TooltipDisplay {
    public static final TooltipDisplay DEFAULT = null;
    public boolean hideTooltip() { throw new AssertionError(); }
    public boolean shows(DataComponentType<?> type) { throw new AssertionError(); }
    public TooltipDisplay withHidden(DataComponentType<?> type, boolean hidden) { throw new AssertionError(); }
}"#,
    ),
    (
        "net/minecraft/world/item/TooltipFlag.java",
        "package net.minecraft.world.item; public interface TooltipFlag { boolean isAdvanced(); boolean isCreative(); }",
    ),
    (
        "net/minecraft/world/item/Item.java",
        r#"package net.minecraft.world.item;
import java.util.function.Consumer;
import net.minecraft.network.chat.Component;
import net.minecraft.world.flag.FeatureFlag;
import net.minecraft.world.item.component.TooltipDisplay;
import net.minecraft.world.level.ItemLike;
public class Item implements ItemLike {
    public static class Properties {
        public Properties() {}
        public Properties durability(int maxDamage) { throw new AssertionError(); }
        public Properties requiredFeatures(FeatureFlag... flags) { throw new AssertionError(); }
    }
    public interface TooltipContext {}
    public Item(Properties properties) {}
    @Override public Item asItem() { return this; }
    public void appendHoverText(ItemStack stack, TooltipContext ctx, TooltipDisplay display, Consumer<Component> out, TooltipFlag flag) {}
}"#,
    ),
    (
        "net/minecraft/world/item/Items.java",
        "package net.minecraft.world.item; public final class Items { public static final Item STONE = null; public static final Item DIRT = null; public static final Item GOLD_INGOT = null; public static final Item DIAMOND_SWORD = null; public static final Item APPLE = null; public static final Item BREAD = null; }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        r#"package net.minecraft.world.item;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.world.level.ItemLike;
public final class ItemStack {
    public ItemStack(ItemLike item) {}
    public static boolean isSameItemSameComponents(ItemStack a, ItemStack b) { throw new AssertionError(); }
    public <T> T get(DataComponentType<? extends T> type) { throw new AssertionError(); }
    public <T> T set(DataComponentType<? super T> type, T value) { throw new AssertionError(); }
    public <T> T remove(DataComponentType<? extends T> type) { throw new AssertionError(); }
    public boolean has(DataComponentType<?> type) { throw new AssertionError(); }
    public int getDamageValue() { throw new AssertionError(); }
    public int getMaxDamage() { throw new AssertionError(); }
    public boolean isDamaged() { throw new AssertionError(); }
}"#,
    ),
    (
        "net/minecraft/world/item/CreativeModeTab.java",
        r#"package net.minecraft.world.item;
import net.minecraft.world.level.ItemLike;
public class CreativeModeTab {
    public enum TabVisibility {
        PARENT_AND_SEARCH_TABS,
        PARENT_TAB_ONLY,
        SEARCH_TAB_ONLY
    }
    public interface Output {
        void accept(ItemStack stack, TabVisibility visibility);
        default void accept(ItemStack stack) { accept(stack, TabVisibility.PARENT_AND_SEARCH_TABS); }
        default void accept(ItemLike item, TabVisibility visibility) { accept(new ItemStack(item), visibility); }
        default void accept(ItemLike item) { accept(new ItemStack(item), TabVisibility.PARENT_AND_SEARCH_TABS); }
    }
}"#,
    ),
    (
        "net/minecraft/world/item/CreativeModeTabs.java",
        "package net.minecraft.world.item; import net.minecraft.resources.ResourceKey; public final class CreativeModeTabs { public static final ResourceKey<CreativeModeTab> FUNCTIONAL_BLOCKS = null; public static final ResourceKey<CreativeModeTab> COMBAT = null; public static final ResourceKey<CreativeModeTab> INGREDIENTS = null; public static final ResourceKey<CreativeModeTab> TOOLS_AND_UTILITIES = null; public static final ResourceKey<CreativeModeTab> FOOD_AND_DRINKS = null; }",
    ),
    (
        "net/fabricmc/fabric/api/event/Event.java",
        "package net.fabricmc.fabric.api.event; public abstract class Event<T> { public abstract void register(T listener); }",
    ),
    (
        "net/fabricmc/fabric/api/creativetab/v1/FabricCreativeModeTabOutput.java",
        r#"package net.fabricmc.fabric.api.creativetab.v1;
import java.util.Collection;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ItemLike;
public class FabricCreativeModeTabOutput implements CreativeModeTab.Output {
    @Override public void accept(ItemStack stack, CreativeModeTab.TabVisibility visibility) { throw new AssertionError(); }
    public void prepend(ItemLike item) { throw new AssertionError(); }
    public void prepend(ItemStack stack, CreativeModeTab.TabVisibility visibility) { throw new AssertionError(); }
    public void insertBefore(ItemLike anchor, Collection<ItemStack> stacks, CreativeModeTab.TabVisibility visibility) { throw new AssertionError(); }
    public void insertBefore(ItemLike anchor, ItemLike... items) { throw new AssertionError(); }
    public void insertBefore(ItemStack anchor, ItemLike... items) { throw new AssertionError(); }
    public void insertAfter(ItemLike anchor, Collection<ItemStack> stacks, CreativeModeTab.TabVisibility visibility) { throw new AssertionError(); }
    public void insertAfter(ItemLike anchor, ItemLike... items) { throw new AssertionError(); }
    public void insertAfter(ItemStack anchor, ItemLike... items) { throw new AssertionError(); }
}"#,
    ),
    (
        "net/fabricmc/fabric/api/creativetab/v1/CreativeModeTabEvents.java",
        r#"package net.fabricmc.fabric.api.creativetab.v1;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.CreativeModeTab;
public final class CreativeModeTabEvents {
    public static final Event<ModifyOutputAll> MODIFY_OUTPUT_ALL = null;
    public interface ModifyOutput { void modifyOutput(FabricCreativeModeTabOutput output); }
    public interface ModifyOutputAll { void modifyOutput(CreativeModeTab tab, FabricCreativeModeTabOutput output); }
    public static Event<ModifyOutput> modifyOutputEvent(ResourceKey<CreativeModeTab> tabKey) { throw new AssertionError(); }
}"#,
    ),
];

const FIXTURE_MOD_SOURCE: &str = r#"package demo;
import java.util.List;
import java.util.function.Consumer;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.creativetab.v1.CreativeModeTabEvents;
import net.minecraft.ChatFormatting;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.item.component.TooltipDisplay;

public final class FixtureMod implements ModInitializer {
    static final class StyledTooltipItem extends Item {
        StyledTooltipItem(Properties props) {
            super(props);
        }

        @Override
        public void appendHoverText(
            ItemStack stack,
            TooltipContext context,
            TooltipDisplay display,
            Consumer<Component> out,
            TooltipFlag flag
        ) {
            out.accept(
                Component.translatable("demo.tooltip.hint", "ok")
                    .withStyle(ChatFormatting.GRAY, ChatFormatting.ITALIC)
            );
            // Arbitrary Object arguments are accepted by TranslatableContents at construction
            // but refused by ComponentSerialization.CODEC -> must be counted, not silently dropped.
            out.accept(Component.translatable("demo.tooltip.unconvertible", new Object()));
        }
    }

    private static Item register(String path, Item item) {
        return Registry.register(
            BuiltInRegistries.ITEM,
            Identifier.fromNamespaceAndPath("demo", path),
            item
        );
    }

    @Override
    public void onInitialize() {
        Item beforeAnchor = register("before_anchor", new Item(new Item.Properties()));
        Item afterAnchor = register("after_anchor", new Item(new Item.Properties()));
        Item followsBefore = register("follows_before", new Item(new Item.Properties()));
        Item tradeItem = register(
            "trade_flag_item",
            new Item(new Item.Properties().requiredFeatures(FeatureFlags.TRADE_REBALANCE))
        );
        Item missingAnchor = register("missing_anchor_item", new Item(new Item.Properties()));
        Item prepended = register("prepended_item", new Item(new Item.Properties()));
        Item combatParent = register("combat_parent_only", new Item(new Item.Properties()));
        Item combatSearch = register("combat_search_only", new Item(new Item.Properties()));
        Item combatAll = register("combat_all_tabs", new Item(new Item.Properties()));
        register("tooltip_item", new StyledTooltipItem(new Item.Properties()));

        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.FUNCTIONAL_BLOCKS).register(entries -> {
            entries.insertBefore(
                Items.DIRT,
                List.of(new ItemStack(beforeAnchor)),
                CreativeModeTab.TabVisibility.PARENT_TAB_ONLY
            );
            entries.insertAfter(
                Items.STONE,
                List.of(new ItemStack(afterAnchor)),
                CreativeModeTab.TabVisibility.SEARCH_TAB_ONLY
            );
            // In Parent, `beforeAnchor` is present so both follow it; in Search, `beforeAnchor`
            // is absent so enabled stacks append to the end. `tradeItem` is filtered unless
            // TRADE_REBALANCE is enabled.
            entries.insertAfter(beforeAnchor, followsBefore, tradeItem);
            // Missing anchor appends to the end of both lists (Fabric 5.0.21 semantics).
            entries.insertBefore(Items.GOLD_INGOT, missingAnchor);
            entries.prepend(prepended);
        });

        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.COMBAT).register(entries -> {
            entries.accept(combatParent, CreativeModeTab.TabVisibility.PARENT_TAB_ONLY);
            entries.accept(combatSearch, CreativeModeTab.TabVisibility.SEARCH_TAB_ONLY);
            entries.accept(combatAll);
        });
    }
}
"#;

fn report_row(java: &HorizonJava) -> serde_json::Value {
    java.lock_client()
        .report_rows()
        .next()
        .expect("one client jar row")
}

fn unfired_sources(row: &serde_json::Value) -> Vec<(String, u64)> {
    row["client"]["unfired_registrations"]
        .as_array()
        .expect("unfired_registrations array")
        .iter()
        .map(|entry| {
            (
                entry["source"].as_str().unwrap().to_owned(),
                entry["count"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn creative_features_use_exact_vanilla_masks_and_refuse_unknowns() {
    assert_eq!(
        crate::runtime::creative_feature_mask(&["minecraft:vanilla".into()]),
        Some(1)
    );
    assert_eq!(
        crate::runtime::creative_feature_mask(&[
            "minecraft:vanilla".into(),
            "minecraft:trade_rebalance".into(),
        ]),
        Some(3)
    );
    assert_eq!(
        crate::runtime::creative_feature_mask(&["example:future".into()]),
        None
    );
}

#[test]
fn javac_fixture_creative_tab_anchors_visibility_flags_and_tooltip_bridge() {
    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.push(("demo/FixtureMod.java", FIXTURE_MOD_SOURCE));
    let jar_bytes = build_javac_jar(
        "client-dispatch",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.FixtureMod"]}}"#,
        &sources,
    );

    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-client.jar".to_owned(), jar_bytes)]);

    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let mut host = TestClientHost::new(&[
                "minecraft:stone",
                "minecraft:dirt",
                "minecraft:gold_ingot",
                "minecraft:iron_sword",
                "demo:before_anchor",
                "demo:after_anchor",
                "demo:follows_before",
                "demo:trade_flag_item",
                "demo:missing_anchor_item",
                "demo:prepended_item",
                "demo:combat_parent_only",
                "demo:combat_search_only",
                "demo:combat_all_tabs",
                "demo:tooltip_item",
            ]);

            // Load client jars through the real client_setup hook (sets game_version = "26.3").
            mod_api::dispatch_client_setup(&mut host);

            let functional_source = "net/fabricmc/fabric/api/creativetab/v1/CreativeModeTabEvents.modifyOutputEvent/minecraft:creative_mode_tab:minecraft:functional_blocks";
            let combat_source = "net/fabricmc/fabric/api/creativetab/v1/CreativeModeTabEvents.modifyOutputEvent/minecraft:creative_mode_tab:minecraft:combat";

            let initial_row = report_row(&java);
            assert_eq!(initial_row["verdict"], "ran-clean");
            assert_eq!(initial_row["unknown_host_call_count"], 0);
            assert_eq!(
                unfired_sources(&initial_row),
                vec![
                    (combat_source.to_owned(), 1),
                    (functional_source.to_owned(), 1),
                ]
            );

            // 1) Functional Blocks with vanilla-only flags: anchors, parent/search visibility,
            //    prepend, missing-anchor fallback, and filtered trade_flag_item — exact Java 25
            //    FabricCreativeModeTabOutput oracle order.
            let edit = host.begin_tab(
                1,
                &["minecraft:stone", "minecraft:dirt"],
                &["minecraft:stone", "minecraft:dirt"],
            );
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:functional_blocks",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![
                    ("demo:prepended_item".to_owned(), 1),
                    ("minecraft:stone".to_owned(), 1),
                    ("demo:before_anchor".to_owned(), 1),
                    ("demo:follows_before".to_owned(), 1),
                    ("minecraft:dirt".to_owned(), 1),
                    ("demo:missing_anchor_item".to_owned(), 1),
                ]
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Search),
                vec![
                    ("demo:prepended_item".to_owned(), 1),
                    ("minecraft:stone".to_owned(), 1),
                    ("demo:after_anchor".to_owned(), 1),
                    ("minecraft:dirt".to_owned(), 1),
                    ("demo:follows_before".to_owned(), 1),
                    ("demo:missing_anchor_item".to_owned(), 1),
                ]
            );
            let after_functional = report_row(&java);
            assert_eq!(after_functional["unknown_host_call_count"], 0);
            assert_eq!(
                unfired_sources(&after_functional),
                vec![(combat_source.to_owned(), 1)]
            );

            // 2) Functional Blocks with TRADE_REBALANCE enabled: `demo:trade_flag_item` is now
            //    included immediately after `demo:follows_before` in both Parent and Search.
            let edit = host.begin_tab(
                2,
                &["minecraft:stone", "minecraft:dirt"],
                &["minecraft:stone", "minecraft:dirt"],
            );
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:functional_blocks",
                vec![
                    "minecraft:vanilla".to_owned(),
                    "minecraft:trade_rebalance".to_owned(),
                ],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![
                    ("demo:prepended_item".to_owned(), 1),
                    ("minecraft:stone".to_owned(), 1),
                    ("demo:before_anchor".to_owned(), 1),
                    ("demo:follows_before".to_owned(), 1),
                    ("demo:trade_flag_item".to_owned(), 1),
                    ("minecraft:dirt".to_owned(), 1),
                    ("demo:missing_anchor_item".to_owned(), 1),
                ]
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Search),
                vec![
                    ("demo:prepended_item".to_owned(), 1),
                    ("minecraft:stone".to_owned(), 1),
                    ("demo:after_anchor".to_owned(), 1),
                    ("minecraft:dirt".to_owned(), 1),
                    ("demo:follows_before".to_owned(), 1),
                    ("demo:trade_flag_item".to_owned(), 1),
                    ("demo:missing_anchor_item".to_owned(), 1),
                ]
            );
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            // 3) Combat tab: parent-only vs search-only vs both visibility, and clears the
            //    remaining unfired creative-tab registration.
            let edit = host.begin_tab(3, &["minecraft:iron_sword"], &["minecraft:iron_sword"]);
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:combat",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![
                    ("minecraft:iron_sword".to_owned(), 1),
                    ("demo:combat_parent_only".to_owned(), 1),
                    ("demo:combat_all_tabs".to_owned(), 1),
                ]
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Search),
                vec![
                    ("minecraft:iron_sword".to_owned(), 1),
                    ("demo:combat_search_only".to_owned(), 1),
                    ("demo:combat_all_tabs".to_owned(), 1),
                ]
            );
            let after_combat = report_row(&java);
            assert_eq!(after_combat["unknown_host_call_count"], 0);
            assert!(unfired_sources(&after_combat).is_empty());

            // 4) Unknown feature flag is refused and counted, leaving the tab untouched.
            let edit = host.begin_tab(4, &["minecraft:stone"], &["minecraft:stone"]);
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:functional_blocks",
                vec!["example:unknown".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![("minecraft:stone".to_owned(), 1)]
            );
            assert_eq!(report_row(&java)["unknown_host_call_count"], 1);

            // 5) Item tooltip: styled translatable line is spliced immediately after line 0
            //    (item name) and before advanced lines; the unconvertible component is NOT
            //    emitted and IS counted (unknown_host_call_count goes 1 -> 2).
            let tooltip_stack = host.stack("demo:tooltip_item");
            let display = TooltipDisplay::for_stack(&tooltip_stack);
            let mut lines = vec![
                (Text::literal("Demo Tooltip Item"), 0xFFFF_FFFF),
                (Text::literal("demo:tooltip_item"), 0xFF55_5555),
            ];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&tooltip_stack),
                    display: &display,
                    advanced: true,
                    creative: false,
                    lines: TooltipLines::new(&mut VecLines(&mut lines)),
                },
            );
            let mut expected_style = TextStyle::default();
            expected_style.color = Some("gray".to_owned());
            expected_style.italic = Some(true);
            let expected_line =
                Text::translate("demo.tooltip.hint", vec![Text::literal("ok")])
                    .with_style(expected_style);
            assert_eq!(
                lines,
                vec![
                    (Text::literal("Demo Tooltip Item"), 0xFFFF_FFFF),
                    (expected_line.clone(), u32::MAX),
                    (Text::literal("demo:tooltip_item"), 0xFF55_5555),
                ]
            );
            assert_eq!(
                report_row(&java)["unknown_host_call_count"],
                2,
                "unconvertible tooltip component must be counted, not silently dropped"
            );

            // 6) `hide_tooltip` skips `appendHoverText` when `creative == false` (vanilla
            //    `ItemStack.getTooltipLines`), and runs it when `creative == true`.
            let mut hidden_stack = host.stack("demo:tooltip_item");
            hidden_stack
                .set_components(
                    [(
                        "minecraft:tooltip_display".into(),
                        Tag::compound(vec![("hide_tooltip", Tag::Byte(1))]),
                    )]
                    .into(),
                )
                .unwrap();
            let hidden_display = TooltipDisplay::for_stack(&hidden_stack);
            let mut hidden_lines = Vec::new();
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&hidden_stack),
                    display: &hidden_display,
                    advanced: false,
                    creative: false,
                    lines: TooltipLines::new(&mut VecLines(&mut hidden_lines)),
                },
            );
            assert!(hidden_lines.is_empty());
            assert_eq!(report_row(&java)["unknown_host_call_count"], 2);

            let mut creative_lines = vec![(Text::literal("Demo Tooltip Item"), 0xFFFF_FFFF)];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&hidden_stack),
                    display: &hidden_display,
                    advanced: false,
                    creative: true,
                    lines: TooltipLines::new(&mut VecLines(&mut creative_lines)),
                },
            );
            assert_eq!(
                creative_lines,
                vec![
                    (Text::literal("Demo Tooltip Item"), 0xFFFF_FFFF),
                    (expected_line, u32::MAX),
                ]
            );
            assert_eq!(report_row(&java)["unknown_host_call_count"], 3);

            // 7) A hovered item the host cannot name: this jar registers items, so its
            //    `appendHoverText` may be the one that would run. It is counted on the jar
            //    (3 -> 4), and the tooltip keeps exactly what Sunrise built, no guessed lines.
            host.hide_item_names.set(true);
            let mut unnamed_lines = vec![(Text::literal("Demo Tooltip Item"), 0xFFFF_FFFF)];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&tooltip_stack),
                    display: &display,
                    advanced: true,
                    creative: false,
                    lines: TooltipLines::new(&mut VecLines(&mut unnamed_lines)),
                },
            );
            host.hide_item_names.set(false);
            assert_eq!(
                unnamed_lines,
                vec![(Text::literal("Demo Tooltip Item"), 0xFFFF_FFFF)]
            );
            assert_eq!(
                report_row(&java)["unknown_host_call_count"],
                4,
                "an unnameable hovered item must be counted on the jar, not skipped silently"
            );

            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

#[test]
fn real_runes_jar_creative_tabs_and_altar_tooltip_through_sunrise_bridge() {
    let Some(path) = std::env::var_os("RUNES_JAR").map(PathBuf::from) else {
        eprintln!("RUNES_JAR is unset; real Runes test skipped");
        return;
    };
    if !path.exists() {
        eprintln!("RUNES_JAR does not exist ({path:?}); real Runes test skipped");
        return;
    }
    // 16 MiB stack so unoptimised debug builds do not hit `frame_bound`'s 2 MiB floor
    // while Runes initializes its nested `bundle-api` jar.
    std::thread::Builder::new()
        .name("real-runes-sunrise-bridge".into())
        .stack_size(16 << 20)
        .spawn(move || run_real_runes_bridge_test(&path))
        .unwrap()
        .join()
        .unwrap();
}

fn run_real_runes_bridge_test(path: &Path) {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("runes.jar")
        .to_owned();
    let jar_bytes = fs::read(path).expect("read RUNES_JAR");

    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![(file_name, jar_bytes)]);

    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let mut host = TestClientHost::new(&[
                "minecraft:crafting_table",
                "minecraft:diamond_sword",
                "runes:crafting_altar",
                "runes:arcane_stone",
                "runes:fire_stone",
                "runes:frost_stone",
                "runes:healing_stone",
                "runes:lightning_stone",
                "runes:soul_stone",
                "runes:small_rune_pouch",
                "runes:medium_rune_pouch",
                "runes:large_rune_pouch",
            ]);

            mod_api::dispatch_client_setup(&mut host);
            assert_eq!(java.lock_client().resident_count(), 1);

            let functional_source = "net/fabricmc/fabric/api/creativetab/v1/CreativeModeTabEvents.modifyOutputEvent/minecraft:creative_mode_tab:minecraft:functional_blocks";
            let combat_source = "net/fabricmc/fabric/api/creativetab/v1/CreativeModeTabEvents.modifyOutputEvent/minecraft:creative_mode_tab:minecraft:combat";

            let before_dispatch = report_row(&java);
            let before_unknown = before_dispatch["unknown_host_call_count"]
                .as_u64()
                .expect("unknown_host_call_count");
            let before_unfired = unfired_sources(&before_dispatch);
            assert!(
                before_unfired.contains(&(functional_source.to_owned(), 1)),
                "expected functional_blocks creative listener before tab build: {before_unfired:?}"
            );
            assert!(
                before_unfired.contains(&(combat_source.to_owned(), 1)),
                "expected combat creative listener before tab build: {before_unfired:?}"
            );

            // Functional Blocks: Runes appends `runes:crafting_altar` after vanilla entries.
            let edit = host.begin_tab(
                10,
                &["minecraft:crafting_table"],
                &["minecraft:crafting_table"],
            );
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:functional_blocks",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            let expected_functional = vec![
                ("minecraft:crafting_table".to_owned(), 1),
                ("runes:crafting_altar".to_owned(), 1),
            ];
            assert_eq!(host.tab_entries(CreativeTabList::Parent), expected_functional);
            assert_eq!(host.tab_entries(CreativeTabList::Search), expected_functional);

            // Combat: Runes appends the six rune stones, then bundled `bundle-api` appends the
            // three rune pouches.
            let edit = host.begin_tab(
                11,
                &["minecraft:diamond_sword"],
                &["minecraft:diamond_sword"],
            );
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:combat",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            let expected_combat = vec![
                ("minecraft:diamond_sword".to_owned(), 1),
                ("runes:arcane_stone".to_owned(), 1),
                ("runes:fire_stone".to_owned(), 1),
                ("runes:frost_stone".to_owned(), 1),
                ("runes:healing_stone".to_owned(), 1),
                ("runes:lightning_stone".to_owned(), 1),
                ("runes:soul_stone".to_owned(), 1),
                ("runes:small_rune_pouch".to_owned(), 1),
                ("runes:medium_rune_pouch".to_owned(), 1),
                ("runes:large_rune_pouch".to_owned(), 1),
            ];
            assert_eq!(host.tab_entries(CreativeTabList::Parent), expected_combat);
            assert_eq!(host.tab_entries(CreativeTabList::Search), expected_combat);

            // Both creative-tab registrations must be cleared from `unfired_registrations`.
            let after_tabs = report_row(&java);
            let after_unfired = unfired_sources(&after_tabs);
            assert!(
                !after_unfired.iter().any(|(s, _)| s == functional_source),
                "functional_blocks listener remained unfired: {after_unfired:?}"
            );
            assert!(
                !after_unfired.iter().any(|(s, _)| s == combat_source),
                "combat listener remained unfired: {after_unfired:?}"
            );

            // Altar item tooltip: `RuneCraftingBlock$1.appendHoverText` appends italic gray
            // `block.runes.crafting_altar.hint` right after the item name.
            let altar = host.stack("runes:crafting_altar");
            let display = TooltipDisplay::for_stack(&altar);
            let mut lines = vec![(Text::literal("Rune Crafting Altar"), 0xFFFF_FFFF)];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&altar),
                    display: &display,
                    advanced: false,
                    creative: false,
                    lines: TooltipLines::new(&mut VecLines(&mut lines)),
                },
            );
            let mut expected_style = TextStyle::default();
            expected_style.color = Some("gray".to_owned());
            expected_style.italic = Some(true);
            let expected_hint =
                Text::translate("block.runes.crafting_altar.hint", Vec::new())
                    .with_style(expected_style);
            assert_eq!(
                lines,
                vec![
                    (Text::literal("Rune Crafting Altar"), 0xFFFF_FFFF),
                    (expected_hint, u32::MAX),
                ]
            );

            let after_all = report_row(&java);
            assert_eq!(
                after_all["unknown_host_call_count"].as_u64(),
                Some(before_unknown),
                "creative tab builds and altar tooltip must add zero unknown host calls"
            );

            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

const EDGE_BRIDGE_SOURCE: &str = r#"package demo;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.creativetab.v1.CreativeModeTabEvents;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

public final class EdgeBridgeMod implements ModInitializer {
    @Override
    public void onInitialize() {
        Item unmapped = Registry.register(
            BuiltInRegistries.ITEM,
            Identifier.fromNamespaceAndPath("demo", "not_in_sunrise"),
            new Item(new Item.Properties())
        );
        Item afterCustom = Registry.register(
            BuiltInRegistries.ITEM,
            Identifier.fromNamespaceAndPath("demo", "after_custom"),
            new Item(new Item.Properties())
        );
        Item allTabExtra = Registry.register(
            BuiltInRegistries.ITEM,
            Identifier.fromNamespaceAndPath("demo", "all_tab_extra"),
            new Item(new Item.Properties())
        );

        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.FUNCTIONAL_BLOCKS).register(entries -> {
            // Duplicate of seeded vanilla entry: vanilla ItemStackLinkedSet keeps first occurrence
            // without error.
            entries.accept(Items.STONE);
            // Unregistered in Sunrise's item table: bridge must count conversion refusal.
            entries.accept(unmapped);
            // Valid item placed after plain `Items.DIRT` (which sits after a custom-component stack).
            entries.insertAfter(Items.DIRT, afterCustom);
        });

        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.COMBAT).register(entries -> {
            // `ItemStack` anchor against a seeded stack with custom components must be counted
            // rather than falsely equating components.
            entries.insertAfter(new ItemStack(Items.STONE), afterCustom);
        });

        CreativeModeTabEvents.MODIFY_OUTPUT_ALL.register((tab, entries) -> {
            entries.accept(allTabExtra);
        });
    }
}
"#;

#[test]
fn bridge_counts_unmapped_stacks_and_opaque_anchors_and_deduplicates_like_vanilla() {
    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.push(("demo/EdgeBridgeMod.java", EDGE_BRIDGE_SOURCE));
    sources.push((
        "net/fabricmc/fabric/api/creativetab/v1/FabricCreativeModeTabOutput.java",
        r#"package net.fabricmc.fabric.api.creativetab.v1;
import java.util.Collection;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ItemLike;
public class FabricCreativeModeTabOutput implements CreativeModeTab.Output {
    @Override public void accept(ItemStack stack, CreativeModeTab.TabVisibility visibility) { throw new AssertionError(); }
    public void prepend(ItemLike item) { throw new AssertionError(); }
    public void prepend(ItemStack stack, CreativeModeTab.TabVisibility visibility) { throw new AssertionError(); }
    public void insertBefore(ItemLike anchor, Collection<ItemStack> stacks, CreativeModeTab.TabVisibility visibility) { throw new AssertionError(); }
    public void insertBefore(ItemLike anchor, ItemLike... items) { throw new AssertionError(); }
    public void insertAfter(ItemLike anchor, Collection<ItemStack> stacks, CreativeModeTab.TabVisibility visibility) { throw new AssertionError(); }
    public void insertAfter(ItemLike anchor, ItemLike... items) { throw new AssertionError(); }
    public void insertAfter(ItemStack anchor, ItemLike... items) { throw new AssertionError(); }
}"#,
    ));
    // Replace the earlier stub entry for FabricCreativeModeTabOutput so javac sees only one.
    let stub_idx = sources
        .iter()
        .position(|(p, _)| {
            *p == "net/fabricmc/fabric/api/creativetab/v1/FabricCreativeModeTabOutput.java"
        })
        .unwrap();
    let replacement = sources.pop().unwrap();
    sources[stub_idx] = replacement;

    let jar_bytes = build_javac_jar(
        "client-dispatch-edges",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.EdgeBridgeMod"]}}"#,
        &sources,
    );

    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-edges.jar".to_owned(), jar_bytes)]);

    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            // Deliberately omit `demo:not_in_sunrise` from Sunrise's item table.
            let mut host = TestClientHost::new(&[
                "minecraft:stone",
                "minecraft:dirt",
                "demo:after_custom",
                "demo:all_tab_extra",
            ]);
            mod_api::dispatch_client_setup(&mut host);
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            // Seed a damaged stone stack + plain dirt so `seed_creative_stacks` preserves custom
            // components on stone while plain dirt remains matchable.
            let mut damaged_stone = host.stack("minecraft:stone");
            damaged_stone.set_damage(3);
            let dirt = host.stack("minecraft:dirt");
            let edit = mod_api::__private::creative_tab_edit(20);
            {
                let mut editor = host.editor.borrow_mut();
                editor.active = Some(edit);
                editor.parent = vec![damaged_stone.clone(), dirt.clone()];
                editor.search = vec![damaged_stone.clone(), dirt.clone()];
            }
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:functional_blocks",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            // `Items.STONE` duplicate adds plain stone beside damaged stone (different components!),
            // `demo:not_in_sunrise` is refused and counted once per list (2 total), `demo:after_custom`
            // lands after `minecraft:dirt`, and `MODIFY_OUTPUT_ALL` appends `demo:all_tab_extra`.
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![
                    ("minecraft:stone".to_owned(), 1),
                    ("minecraft:dirt".to_owned(), 1),
                    ("demo:after_custom".to_owned(), 1),
                    ("minecraft:stone".to_owned(), 1),
                    ("demo:all_tab_extra".to_owned(), 1),
                ]
            );
            assert_eq!(host.editor.borrow().parent[0].damage_component(), Some(3));
            assert_eq!(host.editor.borrow().parent[3].damage_component(), None);
            assert_eq!(
                report_row(&java)["unknown_host_call_count"],
                2,
                "unmapped Sunrise item must be counted once for Parent and once for Search"
            );

            // Duplicate of an identical plain stack is ignored cleanly without counting.
            let edit = host.begin_tab(
                21,
                &["minecraft:stone", "minecraft:dirt"],
                &["minecraft:stone", "minecraft:dirt"],
            );
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:functional_blocks",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![
                    ("minecraft:stone".to_owned(), 1),
                    ("minecraft:dirt".to_owned(), 1),
                    ("demo:after_custom".to_owned(), 1),
                    ("demo:all_tab_extra".to_owned(), 1),
                ]
            );
            // Only the 2 conversion counts for `demo:not_in_sunrise` (2 -> 4); zero duplicate errors.
            assert_eq!(report_row(&java)["unknown_host_call_count"], 4);

            // `ItemStack` anchor against damaged stone (modelled `minecraft:damage` component)
            // proves inequality cleanly with zero new unknown calls, appending after `dirt`.
            let edit = mod_api::__private::creative_tab_edit(22);
            {
                let mut editor = host.editor.borrow_mut();
                editor.active = Some(edit);
                editor.parent = vec![damaged_stone.clone(), dirt.clone()];
                editor.search = vec![damaged_stone, dirt];
            }
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:combat",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![
                    ("minecraft:stone".to_owned(), 1),
                    ("minecraft:dirt".to_owned(), 1),
                    ("demo:after_custom".to_owned(), 1),
                    ("demo:all_tab_extra".to_owned(), 1),
                ]
            );
            assert_eq!(
                report_row(&java)["unknown_host_call_count"],
                4,
                "modelled component difference on ItemStack anchor must add zero unknown calls"
            );

            // `ItemStack` anchor against an unmodelled/opaque component (`minecraft:enchantments`)
            // in Combat must count as unknown instead of falsely equating components.
            let mut opaque_stone = host.stack("minecraft:stone");
            opaque_stone
                .set_components(
                    [(
                        "minecraft:enchantments".into(),
                        Tag::compound(vec![(
                            "levels",
                            Tag::compound(vec![("minecraft:sharpness", Tag::Int(5))]),
                        )]),
                    )]
                    .into(),
                )
                .unwrap();
            let edit = mod_api::__private::creative_tab_edit(23);
            {
                let mut editor = host.editor.borrow_mut();
                editor.active = Some(edit);
                editor.parent = vec![opaque_stone.clone()];
                editor.search = vec![opaque_stone];
            }
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:combat",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert!(
                report_row(&java)["unknown_host_call_count"]
                    .as_u64()
                    .unwrap()
                    > 4,
                "ItemStack anchor matching an opaque-component stack must be counted"
            );

            // Direct boundary check on `convert_tooltip_component`: non-component Value is refused.
            let runtime_host = crate::runtime::StdHost::new();
            let (text, refused) = crate::client::convert_tooltip_component(
                &host,
                &runtime_host,
                &crate::runtime::Value::Int(7),
            );
            assert_eq!(text, None);
            assert!(refused);

            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

const CREATIVE_COMPONENTS_MOD: &str = r#"package demo;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.creativetab.v1.CreativeModeTabEvents;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.util.Unit;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.CustomData;

public final class CreativeComponentsMod implements ModInitializer {
    @Override
    public void onInitialize() {
        DataComponentType<Object> unrep = Registry.register(
            BuiltInRegistries.DATA_COMPONENT_TYPE,
            Identifier.fromNamespaceAndPath("demo", "unrepresentable"),
            new DataComponentType<Object>() {}
        );

        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.COMBAT).register(entries -> {
            ItemStack plain = new ItemStack(Items.DIAMOND_SWORD);

            ItemStack damaged = new ItemStack(Items.DIAMOND_SWORD);
            damaged.set(DataComponents.DAMAGE, 12);

            ItemStack named = new ItemStack(Items.DIAMOND_SWORD);
            named.set(DataComponents.CUSTOM_NAME, Component.literal("Runeblade"));

            ItemStack customData = new ItemStack(Items.DIAMOND_SWORD);
            CompoundTag tag = new CompoundTag();
            tag.putInt("tier", 3);
            CustomData.set(DataComponents.CUSTOM_DATA, customData, tag);

            ItemStack unbreakable = new ItemStack(Items.DIAMOND_SWORD);
            unbreakable.set(DataComponents.UNBREAKABLE, Unit.INSTANCE);

            ItemStack noMaxDamage = new ItemStack(Items.DIAMOND_SWORD);
            noMaxDamage.remove(DataComponents.MAX_DAMAGE);

            // Unrepresentable custom component must be skipped and counted, never emitted as plain.
            ItemStack unrepresentable = new ItemStack(Items.DIAMOND_SWORD);
            unrepresentable.set(unrep, new Object());

            // Exact duplicate of `damaged` must deduplicate cleanly without counting.
            ItemStack damagedDup = new ItemStack(Items.DIAMOND_SWORD);
            damagedDup.set(DataComponents.DAMAGE, 12);

            entries.accept(plain);
            entries.accept(damaged);
            entries.accept(named);
            entries.accept(customData);
            entries.accept(unbreakable);
            entries.accept(noMaxDamage);
            entries.accept(unrepresentable);
            entries.accept(damagedDup);
        });

        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.INGREDIENTS).register(entries -> {
            ItemStack damagedAnchor = new ItemStack(Items.DIAMOND_SWORD);
            damagedAnchor.set(DataComponents.DAMAGE, 12);
            entries.insertAfter(damagedAnchor, Items.APPLE);
        });
    }
}
"#;

#[test]
fn creative_tab_preserves_custom_components_and_skips_unrepresentable_stacks() {
    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.push(("demo/CreativeComponentsMod.java", CREATIVE_COMPONENTS_MOD));
    let jar_bytes = build_javac_jar(
        "client-creative-components",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.CreativeComponentsMod"]}}"#,
        &sources,
    );

    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-components.jar".to_owned(), jar_bytes)]);

    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let mut host = TestClientHost::new(&[
                "minecraft:stone",
                "minecraft:diamond_sword",
                "minecraft:apple",
            ]);
            mod_api::dispatch_client_setup(&mut host);
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            let edit = host.begin_tab(30, &["minecraft:stone"], &["minecraft:stone"]);
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:combat",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );

            // Java 25 oracle: plain, damaged(12), custom_name("Runeblade"), custom_data({tier:3}),
            // unbreakable({}), and !max_damage({}) are all distinct in ItemStackLinkedSet; the
            // duplicate damaged(12) collapses cleanly; the unrepresentable stack is skipped & counted.
            let editor = host.editor.borrow();
            assert_eq!(editor.parent.len(), 7, "parent entries: {:?}", editor.parent);
            assert_eq!(editor.search.len(), 7, "search entries: {:?}", editor.search);
            for list in [&editor.parent, &editor.search] {
                assert_eq!(list[0].item().id(&host), Some("minecraft:stone"));
                assert_eq!(list[1].item().id(&host), Some("minecraft:diamond_sword"));
                assert_eq!(list[1].damage_component(), None);
                assert!(list[1].components().is_none());

                assert_eq!(list[2].item().id(&host), Some("minecraft:diamond_sword"));
                assert_eq!(list[2].damage_component(), Some(12));

                assert_eq!(list[3].item().id(&host), Some("minecraft:diamond_sword"));
                assert!(
                    list[3]
                        .components()
                        .and_then(|c| c.as_nbt().get(&"minecraft:custom_name".into()))
                        .is_some(),
                    "expected minecraft:custom_name on entry 3"
                );
                assert!(!list[3].is_stackable_with(&list[1]));

                assert_eq!(list[4].item().id(&host), Some("minecraft:diamond_sword"));
                assert_eq!(
                    list[4]
                        .components()
                        .and_then(|c| c.as_nbt().get(&"minecraft:custom_data".into()))
                        .and_then(|t| t.get("tier")),
                    Some(&Tag::Int(3))
                );

                assert_eq!(list[5].item().id(&host), Some("minecraft:diamond_sword"));
                assert_eq!(
                    list[5]
                        .components()
                        .and_then(|c| c.as_nbt().get(&"minecraft:unbreakable".into())),
                    Some(&Tag::Compound(mod_api::Compound::new()))
                );

                assert_eq!(list[6].item().id(&host), Some("minecraft:diamond_sword"));
                assert_eq!(
                    list[6]
                        .components()
                        .and_then(|c| c.as_nbt().get(&"!minecraft:max_damage".into())),
                    Some(&Tag::Compound(mod_api::Compound::new()))
                );
            }
            drop(editor);
            assert_eq!(
                report_row(&java)["unknown_host_call_count"],
                2,
                "unrepresentable custom component must be counted once for Parent and once for Search"
            );

            // Seeded damaged sword vs plain sword: `insertAfter(damaged(12), APPLE)` lands
            // directly after the damaged sword and before the plain sword with zero new counts.
            let mut damaged = host.stack("minecraft:diamond_sword");
            damaged.set_damage(12);
            let plain = host.stack("minecraft:diamond_sword");
            let edit = mod_api::__private::creative_tab_edit(31);
            {
                let mut editor = host.editor.borrow_mut();
                editor.active = Some(edit);
                editor.parent = vec![damaged.clone(), plain.clone()];
                editor.search = vec![damaged, plain];
            }
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:ingredients",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![
                    ("minecraft:diamond_sword".to_owned(), 1),
                    ("minecraft:apple".to_owned(), 1),
                    ("minecraft:diamond_sword".to_owned(), 1),
                ]
            );
            assert_eq!(host.editor.borrow().parent[0].damage_component(), Some(12));
            assert_eq!(host.editor.borrow().parent[2].damage_component(), None);
            assert_eq!(report_row(&java)["unknown_host_call_count"], 2);

            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

const TOOLTIP_REAL_STATE_MOD: &str = r#"package demo;
import java.util.function.Consumer;
import net.fabricmc.api.ModInitializer;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.item.component.TooltipDisplay;

public final class TooltipRealStateMod implements ModInitializer {
    static final class StateProbeItem extends Item {
        StateProbeItem(Properties props) {
            super(props);
        }

        @Override
        public void appendHoverText(
            ItemStack stack,
            TooltipContext context,
            TooltipDisplay display,
            Consumer<Component> out,
            TooltipFlag flag
        ) {
            int dmg = stack.getDamageValue();
            boolean unb = stack.has(DataComponents.UNBREAKABLE);
            CustomData cd = stack.get(DataComponents.CUSTOM_DATA);
            int tier = cd != null ? cd.copyTag().getIntOr("tier", -1) : -1;
            boolean hide = display.hideTooltip();
            boolean lore = display.shows(DataComponents.LORE);
            boolean dmgShow = display.shows(DataComponents.DAMAGE);
            boolean banner = display.shows(DataComponents.BANNER_PATTERNS);
            out.accept(Component.literal(
                "dmg=" + dmg
                + "|unb=" + unb
                + "|tier=" + tier
                + "|hide=" + hide
                + "|lore=" + lore
                + "|dmgShow=" + dmgShow
                + "|banner=" + banner
            ));
        }
    }

    static final class OpaqueProbeItem extends Item {
        OpaqueProbeItem(Properties props) {
            super(props);
        }

        @Override
        public void appendHoverText(
            ItemStack stack,
            TooltipContext context,
            TooltipDisplay display,
            Consumer<Component> out,
            TooltipFlag flag
        ) {
            CustomData cd = stack.get(DataComponents.CUSTOM_DATA);
            int tier = cd != null ? cd.copyTag().getIntOr("tier", -1) : -1;
            // Reading an unmodelled component present on the stack must be counted.
            stack.get(DataComponents.ENCHANTMENTS);
            out.accept(Component.literal("tier=" + tier));
        }
    }

    @Override
    public void onInitialize() {
        Registry.register(
            BuiltInRegistries.ITEM,
            Identifier.fromNamespaceAndPath("demo", "probe_item"),
            new StateProbeItem(new Item.Properties().durability(100))
        );
        Registry.register(
            BuiltInRegistries.ITEM,
            Identifier.fromNamespaceAndPath("demo", "opaque_probe_item"),
            new OpaqueProbeItem(new Item.Properties())
        );
    }
}
"#;

#[test]
fn item_tooltip_passes_real_stack_components_and_tooltip_display() {
    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.push(("demo/TooltipRealStateMod.java", TOOLTIP_REAL_STATE_MOD));
    let jar_bytes = build_javac_jar(
        "client-tooltip-real-state",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.TooltipRealStateMod"]}}"#,
        &sources,
    );

    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-tooltip-state.jar".to_owned(), jar_bytes)]);

    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let mut host = TestClientHost::new(&["demo:probe_item", "demo:opaque_probe_item"]);
            mod_api::dispatch_client_setup(&mut host);
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            // A) Real stack with damage, unbreakable, custom_data, and hidden `minecraft:lore`.
            let mut rich = host.stack("demo:probe_item");
            rich.set_damage(7);
            rich.set_components(
                [
                    (
                        "minecraft:unbreakable".into(),
                        Tag::Compound(mod_api::Compound::new()),
                    ),
                    (
                        "minecraft:custom_data".into(),
                        Tag::compound(vec![("tier", Tag::Int(4))]),
                    ),
                    (
                        "minecraft:tooltip_display".into(),
                        Tag::compound(vec![(
                            "hidden_components",
                            Tag::List(vec![Tag::String("minecraft:lore".into())]),
                        )]),
                    ),
                ]
                .into(),
            )
            .unwrap();
            let rich_display = TooltipDisplay::for_stack(&rich);
            let mut lines = vec![(Text::literal("Probe"), 0xFFFF_FFFF)];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&rich),
                    display: &rich_display,
                    advanced: false,
                    creative: false,
                    lines: TooltipLines::new(&mut VecLines(&mut lines)),
                },
            );
            assert_eq!(
                lines,
                vec![
                    (Text::literal("Probe"), 0xFFFF_FFFF),
                    (
                        Text::literal("dmg=7|unb=true|tier=4|hide=false|lore=false|dmgShow=true|banner=true"),
                        u32::MAX
                    ),
                ]
            );
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            // B) Ominous banner implicit TooltipDisplay hides `minecraft:banner_patterns`.
            let plain = host.stack("demo:probe_item");
            let ominous_display = TooltipDisplay::from_component(None, true);
            let mut ominous_lines = vec![(Text::literal("Probe"), 0xFFFF_FFFF)];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&plain),
                    display: &ominous_display,
                    advanced: false,
                    creative: false,
                    lines: TooltipLines::new(&mut VecLines(&mut ominous_lines)),
                },
            );
            assert_eq!(
                ominous_lines,
                vec![
                    (Text::literal("Probe"), 0xFFFF_FFFF),
                    (
                        Text::literal("dmg=0|unb=false|tier=-1|hide=false|lore=true|dmgShow=true|banner=false"),
                        u32::MAX
                    ),
                ]
            );
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            // C) `hide_tooltip: 1b` in creative mode passes `hideTooltip() == true` and
            //    `shows(...) == false` for every component (Java 25 TooltipDisplay semantics).
            let mut hidden = host.stack("demo:probe_item");
            hidden
                .set_components(
                    [(
                        "minecraft:tooltip_display".into(),
                        Tag::compound(vec![("hide_tooltip", Tag::Byte(1))]),
                    )]
                    .into(),
                )
                .unwrap();
            let hidden_display = TooltipDisplay::for_stack(&hidden);
            let mut hidden_lines = vec![(Text::literal("Probe"), 0xFFFF_FFFF)];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&hidden),
                    display: &hidden_display,
                    advanced: false,
                    creative: true,
                    lines: TooltipLines::new(&mut VecLines(&mut hidden_lines)),
                },
            );
            assert_eq!(
                hidden_lines,
                vec![
                    (Text::literal("Probe"), 0xFFFF_FFFF),
                    (
                        Text::literal("dmg=0|unb=false|tier=-1|hide=true|lore=false|dmgShow=false|banner=false"),
                        u32::MAX
                    ),
                ]
            );
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            // D) Stack with an unrepresentable component (`minecraft:enchantments`): modelled
            //    `CUSTOM_DATA` still reads accurately, while reading `ENCHANTMENTS` is counted.
            let mut opaque_stack = host.stack("demo:opaque_probe_item");
            opaque_stack
                .set_components(
                    [
                        (
                            "minecraft:custom_data".into(),
                            Tag::compound(vec![("tier", Tag::Int(9))]),
                        ),
                        (
                            "minecraft:enchantments".into(),
                            Tag::compound(vec![(
                                "levels",
                                Tag::compound(vec![("minecraft:sharpness", Tag::Int(5))]),
                            )]),
                        ),
                    ]
                    .into(),
                )
                .unwrap();
            let opaque_display = TooltipDisplay::for_stack(&opaque_stack);
            let mut opaque_lines = vec![(Text::literal("Opaque"), 0xFFFF_FFFF)];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&opaque_stack),
                    display: &opaque_display,
                    advanced: false,
                    creative: false,
                    lines: TooltipLines::new(&mut VecLines(&mut opaque_lines)),
                },
            );
            assert_eq!(
                opaque_lines,
                vec![
                    (Text::literal("Opaque"), 0xFFFF_FFFF),
                    (Text::literal("tier=9"), u32::MAX),
                ]
            );
            assert_eq!(
                report_row(&java)["unknown_host_call_count"],
                1,
                "reading an unrepresentable component in tooltip must be counted"
            );

            // E) Malformed `minecraft:tooltip_display` tag must be counted rather than silently
            //    falling back to `TooltipDisplay.DEFAULT`.
            let mut bad_td_stack = host.stack("demo:probe_item");
            bad_td_stack
                .set_components(
                    [(
                        "minecraft:tooltip_display".into(),
                        Tag::String("malformed".into()),
                    )]
                    .into(),
                )
                .unwrap();
            let bad_td_display = TooltipDisplay::for_stack(&bad_td_stack);
            let mut bad_td_lines = vec![(Text::literal("Probe"), 0xFFFF_FFFF)];
            mod_api::dispatch_item_tooltip(
                &host,
                mod_api::__private::ItemTooltipCall {
                    player: 0,
                    player_entity_id: 1,
                    stack: ItemStackView::from_stack(&bad_td_stack),
                    display: &bad_td_display,
                    advanced: false,
                    creative: false,
                    lines: TooltipLines::new(&mut VecLines(&mut bad_td_lines)),
                },
            );
            assert!(
                report_row(&java)["unknown_host_call_count"]
                    .as_u64()
                    .unwrap()
                    > 1,
                "malformed tooltip_display must be counted"
            );

            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

/// A tooltip shows every frame, so Sunrise asks `appendHoverText` every frame. Each call's stack,
/// `TooltipDisplay` and whatever the Java code makes must be garbage once its lines are Sunrise
/// `Text`: none may stay a root of the client's heap (#609 round 6 bug G). Before the call ran
/// in a transient scope, every frame left its stack and display rows behind as roots for good.
/// In game that is 2 rows per hovered frame, every one walked by every later collection: after
/// round 6's 15-minute soak the pause was ~110 ms, with one every ~5 s.
///
/// The bound is on work, not time: after warm-up, every collection over 2,000 tooltip frames
/// keeps no more entries and starts from no more root texts than the first one did (plus a
/// little slack for counters), and no recipe row outside a closed scope is added. Each frame's
/// line equals case A of `item_tooltip_passes_real_stack_components_and_tooltip_display`, and
/// nothing counts.
#[test]
fn item_tooltip_every_frame_roots_nothing_and_collections_stay_bounded() {
    /// Entries or root texts a collection may exceed the first one by: counters and freed
    /// ranges settle, they don't climb. On main each frame rooted 2 rows (a stack and a
    /// display), so 2,000 frames rooted ~4,000.
    const SLACK: usize = 32;
    const FRAMES: usize = 2_000;

    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.push(("demo/TooltipRealStateMod.java", TOOLTIP_REAL_STATE_MOD));
    let jar_bytes = build_javac_jar(
        "client-tooltip-every-frame",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.TooltipRealStateMod"]}}"#,
        &sources,
    );
    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-tooltip-frames.jar".to_owned(), jar_bytes)]);
    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    // What one collection walked and kept, and the recipe rows that are roots, right now.
    let measure = |java: &HorizonJava| {
        java.lock_client().with_first_jar_mut(|_, host| {
            let (rows, collectable) = host.recipe_row_counts();
            (
                host.collection_count(),
                host.last_collection().cloned(),
                rows - collectable,
            )
        })
    };
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let mut host = TestClientHost::new(&["demo:probe_item", "demo:opaque_probe_item"]);
            mod_api::dispatch_client_setup(&mut host);
            let mut rich = host.stack("demo:probe_item");
            rich.set_damage(7);
            rich.set_components(
                [
                    (
                        "minecraft:unbreakable".into(),
                        Tag::Compound(mod_api::Compound::new()),
                    ),
                    (
                        "minecraft:custom_data".into(),
                        Tag::compound(vec![("tier", Tag::Int(4))]),
                    ),
                    (
                        "minecraft:tooltip_display".into(),
                        Tag::compound(vec![(
                            "hidden_components",
                            Tag::List(vec![Tag::String("minecraft:lore".into())]),
                        )]),
                    ),
                ]
                .into(),
            )
            .unwrap();
            let display = TooltipDisplay::for_stack(&rich);
            let frame = || {
                let mut lines = vec![(Text::literal("Probe"), 0xFFFF_FFFF)];
                mod_api::dispatch_item_tooltip(
                    &host,
                    mod_api::__private::ItemTooltipCall {
                        player: 0,
                        player_entity_id: 1,
                        stack: ItemStackView::from_stack(&rich),
                        display: &display,
                        advanced: false,
                        creative: false,
                        lines: TooltipLines::new(&mut VecLines(&mut lines)),
                    },
                );
                assert_eq!(
                    lines,
                    vec![
                        (Text::literal("Probe"), 0xFFFF_FFFF),
                        (
                            Text::literal(
                                "dmg=7|unb=true|tier=4|hide=false|lore=false|dmgShow=true|banner=true"
                            ),
                            u32::MAX
                        ),
                    ]
                );
            };
            // Warm up to the first collection: classes initialise and caches settle.
            let start = measure(&java).0;
            let mut warm = 0;
            while measure(&java).0 == start {
                frame();
                warm += 1;
                assert!(warm < FRAMES, "no collection in {FRAMES} tooltip frames");
            }
            let (mut seen, first, rooted) = measure(&java);
            let first = first.expect("a collection ran");
            let mut collections = Vec::new();
            for _ in 0..FRAMES {
                frame();
                let (count, last, _) = measure(&java);
                if count != seen {
                    seen = count;
                    collections.push(last.expect("a collection ran"));
                }
            }
            let (_, _, rooted_after) = measure(&java);
            assert!(
                collections.len() >= 3,
                "only {} collections in {FRAMES} frames",
                collections.len()
            );
            for (i, stats) in collections.iter().enumerate() {
                assert!(
                    stats.entries_after <= first.entries_after + SLACK
                        && stats.roots <= first.roots + SLACK,
                    "collection {i} of {}: kept {} entries from {} root texts; the first kept {} \
                     from {}",
                    collections.len(),
                    stats.entries_after,
                    stats.roots,
                    first.entries_after,
                    first.roots
                );
            }
            assert_eq!(
                rooted_after, rooted,
                "recipe rows outside a closed scope after {FRAMES} tooltip frames"
            );
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);
            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

/// A creative tab's build seeds a runtime stack for every entry Sunrise already has, and the jar's
/// listeners make more. All of them are garbage once the contents are Sunrise stacks: a build
/// may root no recipe row. Before the build ran in a transient scope, every seeded and made
/// stack stayed a root for good. Opening the creative inventory once in game rooted ~8,400
/// stacks, and every later collection walked them (#609 round 6). The same build 100 times
/// gives the first build's lists each time and leaves the rooted rows where the first build did.
#[test]
fn creative_tab_builds_root_no_recipe_rows() {
    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.push(("demo/CreativeComponentsMod.java", CREATIVE_COMPONENTS_MOD));
    let jar_bytes = build_javac_jar(
        "client-creative-rows",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.CreativeComponentsMod"]}}"#,
        &sources,
    );
    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-creative-rows.jar".to_owned(), jar_bytes)]);
    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    let rooted_rows = |java: &HorizonJava| {
        java.lock_client().with_first_jar_mut(|_, host| {
            let (rows, collectable) = host.recipe_row_counts();
            rows - collectable
        })
    };
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let mut host = TestClientHost::new(&[
                "minecraft:stone",
                "minecraft:diamond_sword",
                "minecraft:apple",
            ]);
            mod_api::dispatch_client_setup(&mut host);
            let rooted_at_setup = rooted_rows(&java);
            let mut first: Option<(String, String)> = None;
            for build in 0..100u64 {
                let edit = host.begin_tab(100 + build, &["minecraft:stone"], &["minecraft:stone"]);
                mod_api::dispatch_build_creative_mode_tab_contents(
                    HostSide::Client(&mut host),
                    "minecraft:combat",
                    vec!["minecraft:vanilla".to_owned()],
                    CreativeTabPermissions::Normal,
                    edit,
                );
                // The seven entries of `creative_tab_preserves_custom_components_and_skips_
                // unrepresentable_stacks` in each list, the same every build.
                let lists = {
                    let editor = host.editor.borrow();
                    assert_eq!((editor.parent.len(), editor.search.len()), (7, 7));
                    (
                        format!("{:?}", editor.parent),
                        format!("{:?}", editor.search),
                    )
                };
                match &first {
                    None => first = Some(lists),
                    Some(first) => assert_eq!(&lists, first, "build {build}"),
                }
                assert_eq!(
                    rooted_rows(&java),
                    rooted_at_setup,
                    "recipe rows rooted after build {build}"
                );
            }
            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

const DUPLICATE_REORDER_MOD: &str = r#"package demo;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.creativetab.v1.CreativeModeTabEvents;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.Items;

public final class DuplicateReorderMod implements ModInitializer {
    @Override
    public void onInitialize() {
        // Java 25 + Fabric API 5.0.21 oracle case1:
        // raw [stone, dirt, stone, apple, bread] -> set [stone, dirt, apple, bread]
        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.FUNCTIONAL_BLOCKS).register(entries -> {
            entries.accept(Items.STONE);
            entries.accept(Items.BREAD);
            entries.insertAfter(Items.STONE, Items.APPLE);
        });

        // Java 25 + Fabric API 5.0.21 oracle case2:
        // raw [dirt, stone, dirt, apple] -> set [dirt, stone, apple]
        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.COMBAT).register(entries -> {
            entries.prepend(Items.DIRT);
            entries.insertAfter(Items.DIRT, Items.APPLE);
        });

        // Java 25 + Fabric API 5.0.21 oracle case3:
        // raw [apple, dirt, stone, dirt, bread] -> set [apple, dirt, stone, bread]
        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.INGREDIENTS).register(entries -> {
            entries.insertBefore(Items.STONE, Items.DIRT);
            entries.insertBefore(Items.DIRT, Items.APPLE);
            entries.insertAfter(Items.DIRT, Items.BREAD);
        });

        // Java 25 + Fabric API 5.0.21 oracle case4:
        // raw [stone, dirt, stone, apple] -> set [stone, dirt, apple]
        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.TOOLS_AND_UTILITIES).register(entries -> {
            entries.accept(Items.STONE);
            entries.insertAfter(Items.STONE, Items.APPLE);
        });

        // Java 25 + Fabric API 5.0.21 oracle case4b:
        // raw [stone, bread, dirt, stone, apple] -> set [stone, bread, dirt, apple]
        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.FOOD_AND_DRINKS).register(entries -> {
            entries.insertBefore(Items.DIRT, Items.BREAD);
            entries.accept(Items.STONE);
            entries.insertAfter(Items.STONE, Items.APPLE);
        });
    }
}
"#;

#[test]
fn creative_tab_duplicate_reordering_matches_java25_fabric_oracle() {
    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.push(("demo/DuplicateReorderMod.java", DUPLICATE_REORDER_MOD));
    let jar_bytes = build_javac_jar(
        "client-duplicate-reorder",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.DuplicateReorderMod"]}}"#,
        &sources,
    );

    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-reorder.jar".to_owned(), jar_bytes)]);

    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let mut host = TestClientHost::new(&[
                "minecraft:stone",
                "minecraft:dirt",
                "minecraft:apple",
                "minecraft:bread",
            ]);
            mod_api::dispatch_client_setup(&mut host);
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            let cases: &[(&str, u64, &[&str])] = &[
                (
                    "minecraft:functional_blocks",
                    40,
                    &[
                        "minecraft:stone",
                        "minecraft:dirt",
                        "minecraft:apple",
                        "minecraft:bread",
                    ],
                ),
                (
                    "minecraft:combat",
                    41,
                    &["minecraft:dirt", "minecraft:stone", "minecraft:apple"],
                ),
                (
                    "minecraft:ingredients",
                    42,
                    &[
                        "minecraft:apple",
                        "minecraft:dirt",
                        "minecraft:stone",
                        "minecraft:bread",
                    ],
                ),
                (
                    "minecraft:tools_and_utilities",
                    43,
                    &["minecraft:stone", "minecraft:dirt", "minecraft:apple"],
                ),
                (
                    "minecraft:food_and_drinks",
                    44,
                    &[
                        "minecraft:stone",
                        "minecraft:bread",
                        "minecraft:dirt",
                        "minecraft:apple",
                    ],
                ),
            ];

            for &(tab, id, expected_ids) in cases {
                let edit = host.begin_tab(
                    id,
                    &["minecraft:stone", "minecraft:dirt"],
                    &["minecraft:stone", "minecraft:dirt"],
                );
                mod_api::dispatch_build_creative_mode_tab_contents(
                    HostSide::Client(&mut host),
                    tab,
                    vec!["minecraft:vanilla".to_owned()],
                    CreativeTabPermissions::Normal,
                    edit,
                );
                let expected: Vec<(String, u32)> = expected_ids
                    .iter()
                    .map(|item| ((*item).to_owned(), 1))
                    .collect();
                assert_eq!(
                    host.tab_entries(CreativeTabList::Parent),
                    expected,
                    "Parent mismatch for {tab}"
                );
                assert_eq!(
                    host.tab_entries(CreativeTabList::Search),
                    expected,
                    "Search mismatch for {tab}"
                );
            }
            assert_eq!(
                report_row(&java)["unknown_host_call_count"],
                0,
                "duplicate reconciliation must have zero unknown host calls"
            );

            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

const TOOLTIP_DISPLAY_SET_DEDUPE_MOD: &str = r#"package demo;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.creativetab.v1.CreativeModeTabEvents;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.TooltipDisplay;

public final class TooltipDisplaySetDedupeMod implements ModInitializer {
    @Override
    public void onInitialize() {
        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.COMBAT).register(entries -> {
            TooltipDisplay tdLoreDmg = TooltipDisplay.DEFAULT
                .withHidden(DataComponents.LORE, true)
                .withHidden(DataComponents.DAMAGE, true);
            TooltipDisplay tdDmgLore = TooltipDisplay.DEFAULT
                .withHidden(DataComponents.DAMAGE, true)
                .withHidden(DataComponents.LORE, true);

            // JDK 25 / MC 26.3 oracle: TooltipDisplay.hiddenComponents is a SequencedSet
            // (ReferenceLinkedOpenHashSet), so record equality ignores order and duplicates.
            if (!tdLoreDmg.equals(tdDmgLore)) {
                throw new AssertionError("TooltipDisplay.equals must ignore hiddenComponents order");
            }
            if (tdLoreDmg.equals(TooltipDisplay.DEFAULT)) {
                throw new AssertionError("non-empty TooltipDisplay must not equal DEFAULT");
            }

            ItemStack stackA = new ItemStack(Items.DIAMOND_SWORD);
            stackA.set(DataComponents.TOOLTIP_DISPLAY, tdLoreDmg);
            ItemStack stackB = new ItemStack(Items.DIAMOND_SWORD);
            stackB.set(DataComponents.TOOLTIP_DISPLAY, tdDmgLore);
            if (!ItemStack.isSameItemSameComponents(stackA, stackB)) {
                throw new AssertionError("isSameItemSameComponents must ignore TooltipDisplay hidden order");
            }

            entries.accept(stackA);
            entries.accept(stackB);
        });

        CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.INGREDIENTS).register(entries -> {
            TooltipDisplay tdLoreDmg = TooltipDisplay.DEFAULT
                .withHidden(DataComponents.LORE, true)
                .withHidden(DataComponents.DAMAGE, true);
            ItemStack anchor = new ItemStack(Items.DIAMOND_SWORD);
            anchor.set(DataComponents.TOOLTIP_DISPLAY, tdLoreDmg);
            // Must match a seeded stack whose NBT had reordered/duplicate/unqualified ids,
            // AND accepting `anchor` must dedupe against that seeded stack cleanly.
            entries.insertAfter(anchor, Items.APPLE);
            entries.accept(anchor);
        });
    }
}
"#;

#[test]
fn creative_tab_dedupes_tooltip_display_hidden_components_as_set_matching_jdk25() {
    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.push((
        "demo/TooltipDisplaySetDedupeMod.java",
        TOOLTIP_DISPLAY_SET_DEDUPE_MOD,
    ));
    let jar_bytes = build_javac_jar(
        "client-tooltip-display-set-dedupe",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.TooltipDisplaySetDedupeMod"]}}"#,
        &sources,
    );

    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-td-dedupe.jar".to_owned(), jar_bytes)]);

    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let mut host = TestClientHost::new(&[
                "minecraft:stone",
                "minecraft:diamond_sword",
                "minecraft:apple",
            ]);
            mod_api::dispatch_client_setup(&mut host);
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            // 1) Two newly added stacks whose `tooltip_display.hidden_components` differ only in
            //    order (`[lore, damage]` vs `[damage, lore]`) dedupe to ONE entry keeping the
            //    first occurrence's insertion order (JDK 25 ItemStackLinkedSet oracle).
            let edit = host.begin_tab(50, &["minecraft:stone"], &["minecraft:stone"]);
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:combat",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            let expected_hidden = Tag::List(vec![
                Tag::String("minecraft:lore".into()),
                Tag::String("minecraft:damage".into()),
            ]);
            {
                let editor = host.editor.borrow();
                for list in [&editor.parent, &editor.search] {
                    assert_eq!(
                        list.len(),
                        2,
                        "reordered TooltipDisplay stacks must dedupe to one entry: {list:?}"
                    );
                    assert_eq!(list[0].item().id(&host), Some("minecraft:stone"));
                    assert_eq!(list[1].item().id(&host), Some("minecraft:diamond_sword"));
                    assert_eq!(
                        list[1]
                            .components()
                            .and_then(|c| c.as_nbt().get(&"minecraft:tooltip_display".into()))
                            .and_then(|t| t.get("hidden_components")),
                        Some(&expected_hidden)
                    );
                }
            }
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            // 2) Seeded Sunrise stack with duplicate + unqualified ids (`["damage", "minecraft:lore",
            //    "minecraft:damage", "minecraft:lore"]`) matches `[lore, damage]` as an ItemStack
            //    anchor AND dedupes when accepted again — exact JDK 25 CODEC + Set oracle.
            let mut seeded_dup = host.stack("minecraft:diamond_sword");
            seeded_dup
                .set_components(
                    [(
                        "minecraft:tooltip_display".into(),
                        Tag::compound(vec![(
                            "hidden_components",
                            Tag::List(vec![
                                Tag::String("damage".into()),
                                Tag::String("minecraft:lore".into()),
                                Tag::String("minecraft:damage".into()),
                                Tag::String("minecraft:lore".into()),
                            ]),
                        )]),
                    )]
                    .into(),
                )
                .unwrap();
            let stone = host.stack("minecraft:stone");
            let edit = mod_api::__private::creative_tab_edit(51);
            {
                let mut editor = host.editor.borrow_mut();
                editor.active = Some(edit);
                editor.parent = vec![seeded_dup.clone(), stone.clone()];
                editor.search = vec![seeded_dup, stone];
            }
            mod_api::dispatch_build_creative_mode_tab_contents(
                HostSide::Client(&mut host),
                "minecraft:ingredients",
                vec!["minecraft:vanilla".to_owned()],
                CreativeTabPermissions::Normal,
                edit,
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Parent),
                vec![
                    ("minecraft:diamond_sword".to_owned(), 1),
                    ("minecraft:apple".to_owned(), 1),
                    ("minecraft:stone".to_owned(), 1),
                ]
            );
            assert_eq!(
                host.tab_entries(CreativeTabList::Search),
                vec![
                    ("minecraft:diamond_sword".to_owned(), 1),
                    ("minecraft:apple".to_owned(), 1),
                    ("minecraft:stone".to_owned(), 1),
                ]
            );
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);

            Ok::<_, ()>(())
        })
        .unwrap();
    });
}

/// Extra stubs for a client jar that plays level sounds.
const CLIENT_SOUND_STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/sounds/SoundEvent.java",
        "package net.minecraft.sounds; import net.minecraft.resources.Identifier; public final class SoundEvent { public static SoundEvent createVariableRangeEvent(Identifier id) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/sounds/SoundSource.java",
        "package net.minecraft.sounds; public enum SoundSource { MASTER, MUSIC, RECORDS, WEATHER, BLOCKS, HOSTILE, NEUTRAL, PLAYERS, AMBIENT, VOICE, UI }",
    ),
    (
        "net/minecraft/world/entity/Entity.java",
        "package net.minecraft.world.entity; public class Entity {}",
    ),
    (
        "net/minecraft/world/level/Level.java",
        "package net.minecraft.world.level; import net.minecraft.sounds.SoundEvent; import net.minecraft.sounds.SoundSource; import net.minecraft.world.entity.Entity; public abstract class Level { public void playLocalSound(double x, double y, double z, SoundEvent s, SoundSource src, float v, float p, boolean delay) {} public void playSeededSound(Entity except, double x, double y, double z, SoundEvent s, SoundSource src, float v, float p, long seed) {} }",
    ),
    (
        "net/minecraft/client/multiplayer/ClientLevel.java",
        "package net.minecraft.client.multiplayer; import net.minecraft.world.level.Level; public class ClientLevel extends Level {}",
    ),
];

const SOUND_TOOLTIP_MOD: &str = r#"package demo;
import java.util.function.Consumer;
import net.fabricmc.api.ModInitializer;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.item.component.TooltipDisplay;

public final class SoundTooltipMod implements ModInitializer {
    public static final class ChimeItem extends Item {
        private ClientLevel level;

        public ChimeItem(Properties props) {
            super(props);
        }

        // No client callback hands a jar a `ClientLevel` yet (outside a frame
        // `Minecraft.level` is null), so the test gives the item one directly.
        public void hold(ClientLevel level) {
            this.level = level;
        }

        @Override
        public void appendHoverText(
            ItemStack stack,
            TooltipContext context,
            TooltipDisplay display,
            Consumer<Component> out,
            TooltipFlag flag
        ) {
            SoundEvent chime = SoundEvent.createVariableRangeEvent(
                Identifier.fromNamespaceAndPath("demo", "chime"));
            // Draws `Level.random` at the call.
            level.playLocalSound(1.0, 2.0, 3.0, chime, SoundSource.BLOCKS, 0.5f, 1.5f, false);
            // `except == minecraft.player` (both null): plays with an explicit seed.
            level.playSeededSound(null, 4.0, 5.0, 6.0, chime, SoundSource.BLOCKS, 1.0f, 1.0f, 99L);
            out.accept(Component.literal("chimed"));
        }
    }

    @Override
    public void onInitialize() {
        Registry.register(
            BuiltInRegistries.ITEM,
            Identifier.fromNamespaceAndPath("demo", "chime_item"),
            new ChimeItem(new Item.Properties())
        );
    }
}
"#;

/// Client sounds a jar's Java code records during a client callback reach Sunrise's
/// `ClientHost::play_local_sound` through the real hook dispatch, or are counted on the jar:
/// never recorded and dropped.
#[test]
fn item_tooltip_drains_recorded_client_sounds_into_play_local_sound() {
    let mut sources = CLIENT_DISPATCH_STUBS.to_vec();
    sources.extend_from_slice(CLIENT_SOUND_STUBS);
    sources.push(("demo/SoundTooltipMod.java", SOUND_TOOLTIP_MOD));
    let jar_bytes = build_javac_jar(
        "client-tooltip-sounds",
        r#"{"schemaVersion":1,"id":"demo","version":"1.0","entrypoints":{"main":["demo.SoundTooltipMod"]}}"#,
        &sources,
    );

    let java = Arc::new(HorizonJava::default());
    java.lock_client()
        .set_fixture_files(vec![("demo-sounds.jar".to_owned(), jar_bytes)]);

    let mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(SharedHorizonJava(Arc::clone(&java))),
    )];
    mod_api::testing::with_mods(mods.into(), || {
        mod_api::testing::rebuild_parts(|| {
            let host = {
                let mut host = TestClientHost::new(&["demo:chime_item"]);
                mod_api::dispatch_client_setup(&mut host);
                host
            };
            assert_eq!(report_row(&java)["unknown_host_call_count"], 0);
            {
                let mut client = java.lock_client();
                let (jar, runtime) = client.resident(0);
                let item = runtime
                    .observed_registry_entry("minecraft:item", "demo:chime_item")
                    .expect("chime item registered")
                    .value
                    .clone();
                jar.invoke_virtual_with_args(
                    "demo/SoundTooltipMod$ChimeItem",
                    "hold",
                    "(Lnet/minecraft/client/multiplayer/ClientLevel;)V",
                    item,
                    &[crate::runtime::StdHost::client_level_handle()],
                    runtime,
                )
                .expect("hand the item a client level");
            }
            let tooltip = |host: &TestClientHost| {
                let stack = host.stack("demo:chime_item");
                let display = TooltipDisplay::for_stack(&stack);
                let mut lines = vec![(Text::literal("Chime"), 0xFFFF_FFFF)];
                mod_api::dispatch_item_tooltip(
                    host,
                    mod_api::__private::ItemTooltipCall {
                        player: 0,
                        player_entity_id: 1,
                        stack: ItemStackView::from_stack(&stack),
                        display: &display,
                        advanced: false,
                        creative: false,
                        lines: TooltipLines::new(&mut VecLines(&mut lines)),
                    },
                );
                lines
            };

            // 1. As the client owner runs: no client draw is deferred to Sunrise, whose
            //    `play_local_sound` draws its seed later and from another stream. The
            //    `playLocalSound` counts at the call (its `Level.random` draw is one Decaf
            //    cannot make); the explicit-seed sound is recorded, drained, and counted.
            let lines = tooltip(&host);
            assert_eq!(lines[1], (Text::literal("chimed"), u32::MAX));
            assert!(host.local_sounds.borrow().is_empty());
            assert_eq!(report_row(&java)["unknown_host_call_count"], 2);
            {
                let mut client = java.lock_client();
                let (_, runtime) = client.resident(0);
                assert!(
                    runtime.played_sounds().is_empty(),
                    "drained, not left behind"
                );
                assert!(!runtime.sound_stream_known("client/level#random"));
            }

            // 2. A new client level starts fresh streams; with the draw deferred to Sunrise
            //    (as a host that could take it would), the local sound reaches
            //    `play_local_sound` through the tooltip hook.
            java.lock_client().level_loaded();
            java.lock_client()
                .resident(0)
                .1
                .set_host_deferred_client_sound_seeds(true);
            let lines = tooltip(&host);
            assert_eq!(lines[1], (Text::literal("chimed"), u32::MAX));
            assert_eq!(
                *host.local_sounds.borrow(),
                vec![(
                    mod_api::SoundPlay::at(
                        "demo:chime",
                        mod_api::SoundSource::Blocks,
                        [1.0, 2.0, 3.0]
                    )
                    .with_volume(0.5)
                    .with_pitch(1.5),
                    0,
                )]
            );
            // The explicit seed is counted again.
            assert_eq!(report_row(&java)["unknown_host_call_count"], 3);
            Ok::<_, ()>(())
        })
        .unwrap();
    });
}
