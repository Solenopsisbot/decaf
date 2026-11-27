# Java item parts: a jar `Item`'s own overrides in Sunrise

Status: design for BUGS-3d34-round2 #2 (Runes' rune pouches act as plain items). Steps 3-6 are
built (`crates/horizon-native-java/src/item_parts.rs`, `mod/src/item_parts.rs`; tests
`fixture_item_parts` against a JDK 25 oracle and `fixture_item_parts_runes` on the real jar);
steps 1-2 are built by #671 (`jar_components.rs`, `bootstrap_mixins.rs`), whose recorded
`Item.Properties.component` objects are the item parts' jar-item prototypes; steps 7-8 are
built too (`client_item_parts.rs`, `mod/src/client_item_parts.rs`; tests
`fixture_client_item_parts` against a JDK 25 / client 26.3 oracle and
`fixture_runes_client_items` on the real jar), on a client bootstrap pass that runs in the
client's own measured order (`bootstrap_clinit_order_26_3_client.txt`). Bugs #3 (pouch recipes)
and #4 (pouch lore) are fixed separately, in the bake and in the runtime's `Item.Properties`
model.

## What the jar defines

Read from `runes-fabric-1.3.2+26.3.jar` and its nested `bundle-api-5.0.0+26.3-fabric.jar` with
JDK 25's `javap -c -p`:

- The pouches are `com.github.theredbrain.bundleapi.item.CustomBundleItem`, which **extends
  `net.minecraft.world.item.Item`, not `BundleItem`**. It overrides `overrideStackedOnOther`,
  `overrideOtherStackedOnMe`, `use`, `isBarVisible`, `getBarWidth`, `getBarColor`,
  `getTooltipImage` and `onDestroyed`, and keeps an item tag filter (`#runes:runes`) and an empty
  description in fields.
- The contents are a component type of the jar's own, `bundleapi:custom_bundle_contents`, which a
  Mixin registers into `BuiltInRegistries.DATA_COMPONENT_TYPE` (`DataComponentTypesMixin`). Its
  persistent codec is a record, `{"content": [ItemStackTemplate...], "size_multiplier": int}`
  (`size_multiplier` optional, default 1), not a bare list. Each pouch's `Item.Properties` sets
  it with `size_multiplier` 4, 8 or 12.
- Its click policy is not vanilla 26.3's. `overrideStackedOnOther` returns `false` unless the
  action is `ClickAction.SECONDARY`, then takes one out (empty slot) or inserts the slot's stack
  through `Builder.add(Slot, Player)`. Vanilla 26.3's `BundleItem` inserts on a **primary** click
  and plays a fail sound when nothing fits.
- The item model `assets/runes/items/*_rune_pouch.json` is a `range_dispatch` on
  `bundleapi:custom_bundle/fullness`, a property the jar's client Mixin
  (`NumericPropertiesMixin`) adds to `RangeSelectItemModelProperties.ID_MAPPER`.
- The tooltip image is the jar's `CustomBundleTooltipData`, which its `TooltipComponentMixin`
  turns into its own `CustomBundleTooltipComponent` (a `ClientTooltipComponent` that draws with
  `GuiGraphics`).

## Why neither of Sunrise's bundles fits

- **Vanilla's `minecraft:bundle_contents`** (`BundleConfig::vanilla`) has a capacity of 1, no
  filter, and vanilla's primary-click insert. A pouch declared as a vanilla bundle would take
  non-rune items, hold a quarter (or less) of what it should, and answer a left click that Fabric
  answers as a plain swap.
- **`BundleConfig::custom(component, multiplier).with_filter_tag(tag)`** has the right capacity
  and filter, but it reads the component as a bare `Tag::List` of stacks
  (`BundleReadError::UnsupportedFormat` otherwise). The jar's record shape doesn't fit, and its
  helpers are data operations only. The click policy would still be the part's to write, and here
  that policy is the jar's bytecode.

Mapping the pouches onto either one would mean recognising bundle-api's classes by name, which the
runtime may not do, and the answers would still differ from Fabric's. So the general path is to
run the jar's own overrides.

## The bridge

Each piece below is general: it applies to any jar item whose class chain overrides one of these
methods, and nothing in it names a mod.

1. **Component types.** Every `minecraft:data_component_type` a jar registers with a persistent
   codec is registered with Sunrise through `Hooks::component_types` (NBT component types). The
   bake writes `served_by = "horizon_java"` into the packages of the namespaces those types are in,
   so the SDK's `NamespaceAuthority` lets `horizon_java` register them. Values are stored as the
   NBT the jar's own codec encodes, run through the DFU models (#623 added encode).
   *Built:* the runtime keeps each type's persistent codec (`declarations_content.rs`), encodes
   with `NbtOps` semantics (`dfu_codec::encode_value_nbt`, checked against JDK 25 in
   `fixture_jar_component_types.rs`) and lists the registered types
   (`StdHost::jar_component_types`, the probe's `content.component_types`); the mod registers
   them in `Hooks::component_types` (`mod/src/content.rs`), and `mod/sunrise-mod.toml` sets
   `[native] serves_content = true` so Sunrise accepts the bake's `served_by`. A Mixin
   `@Inject(method = "<clinit>", at = @At("TAIL"))` into a game class (Bundle API's
   `DataComponentTypesMixin`) runs in one pass over every loaded jar (its own mods and nested
   ones), before any jar's entrypoint, in the order that side's bootstrap finishes those classes
   (`bootstrap_mixins.rs`; `bootstrap_clinit_order_26_3.txt` for the server and
   `bootstrap_clinit_order_26_3_client.txt` for the client, the real client's `Main` up to
   where Fabric Loader 0.20.1 starts the entrypoints in `Minecraft.<init>` (just before the
   last `Thread.currentThread()` call ahead of the "Backend library: {}" log line), both from
   `tools/trace_bootstrap_clinit_order.py`, run with no network under `unshare -rn`). A `TAIL`
   into a static `()V` method the bootstrap ends once (`RangeSelectItemModelProperties
   .bootstrap()`) runs at that method's turn; any other shape that would apply is counted (a
   name-only selector on an overloaded name, a `TAIL` into a static method with arguments or one
   the bootstrap never reaches), and so is a registration into a built-in registry at a point
   after `BuiltInRegistries.freeze()` (vanilla refuses it; Fabric API's registry sync delays the
   freeze).
2. **Default components.** The bake writes the item's component patch for jar types too (here
   `{"bundleapi:custom_bundle_contents": {"content": [], "size_multiplier": 4}}`). That has to
   wait for step 1, because Sunrise refuses an item declaration with an unregistered component
   type, and with it the whole package.
   *Built:* `NativeJar::declare_jar_component_defaults` adds `component:<type>` to each item's
   declaration in the JSON Sunrise's `json_to_tag` reads back as exactly that NBT
   (`jar_components::content_json`); a default it can't carry exactly is counted
   (`DataComponentType#codec`) and left out, and the bake drops (with a reason) any item
   component whose type is neither vanilla nor one the runtime registered.
3. **One `ItemPart` per jar item whose class overrides a hook**, attached the way
   `JavaBlockUsePart` is attached to jar blocks (`mod/src/content.rs`
   `attachable_registrations`):
   - `item_stacked_on_other` (`CarriedOnOther`: `overrideStackedOnOther`; `OtherStackedOnMe`:
     `overrideOtherStackedOnMe`);
   - `use_item` (`use`);
   - `item_bar` (`isBarVisible`, `getBarWidth`, `getBarColor`);
   - `tooltip_image` (`getTooltipImage`), with a client factory in
     `register_client_tooltip_component_factories`;
   - `item_destroyed` (`onDestroyed`).
   The live-menu bridge's `try_item_click_behaviour_override` (`live_menu.rs`) would then run the
   same Java call instead of counting it.
4. **An `ItemStack` façade over the Sunrise stack.** `get`, `set`, `has` and `remove` of a
   component: a jar type through its codec, and a vanilla type through the codec models the
   runtime has (counted otherwise). `getCount`, `getMaxStackSize`, `isEmpty`, `split`,
   `copyWithCount`, `isSameItemSameComponents` and `is(TagKey)` answer from the stack and the
   server's tags. Writes come back as the part's `StackedOnOtherResult` `carried` and `clicked`
   edits.
5. **A `Slot` façade over the clicked slot.** `getItem`, `safeInsert`, `tryRemove`, `mayPlace`,
   `mayPickup`, `allowModification` and `getMaxStackSize(ItemStack)` answer from the slot's rules
   (`menu_slot_rules` / `MenuRef`'s slot reads). A rule the hook can't read for that slot counts.
   `ClickedStackResult::Take` is `safeTake`, and `Set` is a direct `Slot.set`.
6. **A `Player` façade** for what the overrides call: `playSound` (the host's sound), `drop`
   (`use`'s `dropAllBundledItems`) and `awardStat`.
7. **The item model property.** The jar's client Mixin into `RangeSelectItemModelProperties`
   adds `bundleapi:custom_bundle/fullness` → `CustomBundleFullnessProperty.CODEC`. The bridge
   turns that registration into `register_range_select_item_model_property`, and evaluates it by
   running the property class's `get(ItemStack, ClientLevel, ItemOwner, int)` on the client
   runtime.
   *Built:* the `@Shadow` of the private `ID_MAPPER` reads as a client handle whose
   `LateBoundIdMapper.put` is recorded (`client_registrations.rs`;
   `StdHost::client_range_select_properties`; a `minecraft:` id, a `null` codec or a codec
   bound to another id counts). The mod registers each id; Sunrise's decoder runs the jar's
   `MapCodec` on the node map (`NativeJar::decode_range_select_property`, as `dispatchMap`
   reads it) and the bound callback runs `get` on the item-part `ItemStack` façade
   (`NativeJar::range_select_property_value`; a level or holder present is an object whose
   every member counts, an absent one `null`). A property that counts reads `0.0`, as an
   unregistered one does; a node the codec refuses rejects the item definition, as the game's
   decoding does.
8. **The tooltip component.** The jar's `TooltipComponentMixin` maps its `TooltipComponent`
   record to its `ClientTooltipComponent`, and the factory runs `getHeight`, `getWidth` and
   `extractImage` (26.3's name for `renderImage`) through the client screen façade that already
   draws jar menu screens (`menu_screen.rs`).
   *Built:* `ItemPart::tooltip_image` runs the item's own `getTooltipImage`
   (`NativeJar::item_tooltip_image`), then the jar's cancellable `HEAD` callbacks into
   `ClientTooltipComponent.create` in Mixin's apply order (`NativeJar::client_tooltip_component`;
   none cancelling is vanilla refusing the data: counted; a name-only selector on the overloaded
   `create` counts too), then measures the component
   (`NativeJar::tooltip_component_size`) and draws it once in its own box, and answers a custom
   `TooltipComponent` only when all of that was exact (otherwise vanilla's answer, no image,
   stands: never an empty box). The factory's `draw_image` runs `extractImage`
   (`NativeJar::draw_tooltip_component_image`) and paints its recorded draws.
   *The pouch's own draws* (`client_font.rs`, `client_surface.rs`; tests `fixture_client_font`
   against a JDK 25 / client 26.3 oracle, and `fixture_runes_client_items`, whose empty, partly
   full and full pouches now answer images with every draw exact against Bundle API's own
   component on the 26.3 client): `Font.split` is 26.3's `StringSplitter.splitLines` over the
   frame's glyph table for plain text, else the embedder's answer for that exact text, else
   counted with its question recorded; `Font.width` likewise in a screen frame; `textWithWordWrap`
   splits and draws each line; the 26.3 whole-sprite `blitSprite` overloads on `GUI_TEXTURED`
   record `DrawOp::Sprite`, which Sunrise's `GuiDraw::Sprite` paints as the sprite's
   `gui.scaling` says (`hud_layout.rs` `paint_with` → `ui::draw_whole_sprite`, at the pin);
   `item` / `itemDecorations` of a runtime stack record `DrawOp::Stack`.
   *In Sunrise* the tooltip's font is the game's: Sunrise has no glyph table, so the mod asks
   each text the component measures or wraps of `ClientTextClientHost::text_width` /
   `text_split` (vanilla's `Font.width` / `Font.split`) and runs it again with the answers
   (`NativeJar::tooltip_image_with_font`). A discarded run's counts are rewound only when all it
   counted was the font's refusals (`ClientFrameOutput::font_refusals`); a run that counted
   anything else keeps its counts and the image is refused, since a counted call's placeholder
   can outlive the run (a `<clinit>` storing it in a static). A rune slot paints as 26.3's
   `itemDecorations` does (offsets 16-40: `itemBar`, `itemCooldown`, then `itemCount`):
   `DrawContext::item_stack` draws the model, bar and cooldown, and the count text follows at
   `itemCount`'s place, `x + 19 - 2 - font.width(s)`, `y + 9`, white with a shadow.
   An embedder hands the game's prototype values of every item a jar component's stored contents
   name (`NativeJar::record_item_prototype`; the mod seeds each registered item id in the NBT),
   since the type's codec reads them while it decodes.

**Counted, failed and exact (BUGS-3d34-round4 #2, #3).** A call that *counted* leaves Sunrise's
vanilla answer standing (counted on the jar). A call that *failed* (Java threw, ran out of
budget, or hit a host call the runtime refused; `ItemCallReport::failure`, or the runtime's own
error) has no answer at all: the bridge logs it, keeps it in the row's `block_use.failures` and
counts `item_stacked_on_other:failed` / `use_item:failed` / `item_destroyed:failed`, then
refuses a click (handled, nothing moves) and answers a use `FAIL`. It never hands the click to
Sunrise's vanilla swap as if the override had returned `false` (round 4's pouch onto cobblestone
swapped that way, silently: a failed call's runtime rolls back with the host calls it counted).
What failed there was `Item.canFitInsideContainerItems()` on a vanilla item, which the runtime
now answers as 26.3 does (`Item`'s `true`, `false` for the 17 shulker boxes only: a Java 25
capture over every registered item, `vanilla_item_data.rs`).

A bundle's `use` drops its contents, then draws `level.getRandom().nextFloat()` for its sound's
pitch. Vanilla's drop draws only entity randoms, but Sunrise carries the drop out after the call,
through the swing, `item_toss` and `entity_join_level`, any of whose listeners may draw the level
stream, where vanilla would run them before that draw; the SDK can't tell that none does (its
only query says whether *any* mod listens, and this mod always listens to `entity_join_level`).
So the draw counts (`live_menu::draws_no_level_random` has the evidence), and the use stops
there, as the altar's take does (#665's rule (b)): the runtime says where the first counted draw
fell (`CountedDrawStop`), and `server_use` applies what came before it, the cleared pouch and
each drop with its stats (`player_drop_plan`), counts the draw, and carries out nothing after it
(no sound, no `ITEM_USED`, `FAIL` as the result), provided the held stack wasn't written after
the draw. The pouch empties; its drop-contents sound doesn't play.

Tests, in the repo's usual pattern: javac fixtures for each façade with JDK 25 / game 26.3
expected rows, then a real-jar test (`RUNES_JAR`) that inserts runes into a pouch through
`overrideOtherStackedOnMe` and checks the encoded component, the bar width and colour, and the
fullness property against a Fabric run.

## Upstream

None of the eight steps needs a new Sunrise API at the `3d34bbe63d` SDK, as far as these reads
show: the item-part hooks, NBT component types, served-namespace authority, item model property
registration and tooltip factories are all there. One assumption needed checking when step 5
was built; it is settled below. The player inventory "has no registered type" for `menu_slot_rules`, so the part must be
able to read the clicked inventory slot's `mayPlace` and `getMaxStackSize(stack)` from the
`item_stacked_on_other` host. If it can't, the smallest addition is a read-only
`StackedOnOther::slot_rules(&ItemStack) -> { may_place, max_stack_size, may_pickup }` on the event,
answered by the slot the click resolved.

The upstream gap these bugs did show is the unread `fabric:load_conditions` key
([mod-plan.md](mod-plan.md), "Fork seams needed").

### A tooltip image's stacks

No addition is needed at the pin. 26.3's `itemDecorations(font, stack, x, y, text)` draws the
bar, then the cooldown, then the count (offsets 16-40), and `DrawContext::item_stack` (SDK
`host/draw.rs:106`, at the pin Sunrise's `native_item_stack`,
`sunrise-client/src/game/native_hooks/base.rs:4394`) draws the model, the bar and the cooldown with
an empty count into the batch the GUI ops share, so the count text drawn after it is vanilla's
order.

The pin also has `DrawContext::gui_draw_item_with(stack, at, text)` (SDK `host/draw.rs:291`,
Sunrise's `native_gui_draw_item_with`): the model and the whole of `itemDecorations`, count
included. The mod keeps `item_stack` and its own count text because `item_stack` answers whether
it drew (false for an unknown item or an empty stack), which the mod counts and refuses on;
`gui_draw_item_with` answers nothing. The cost: both end in Sunrise's `draw_item_decorations`,
which draws another Sunrise mod's item decorator (`native_item_decorations`) last, after the
count; with `item_stack` that decorator is drawn before the mod's count text, so where the two
overlap the count lands on top of it. Vanilla has no such decorator, so this only differs with
another Sunrise mod's slot decorations on a stack in a jar's tooltip image.

Sunrise main has since added `GuiCanvas::item_decorations(stack, at, count)` through
`DrawContext::draw_gui` (`sunrise-mod-vocab/src/client/gui_canvas.rs:214`), the path once
the pin moves past it: one call for the whole of `itemDecorations`, count included, that can refuse.

### Widths Sunrise answers

A `FontQuestion::Width` carries no summing order (`client_font.rs` `WidthOrder`), and the mod
answers each with `ClientTextClientHost::text_width`, the game's `Font.width(FormattedText)`,
which sums in the text's own order. A width the runtime takes in visual order (a split line's,
`centeredText` of a component's `getVisualOrderText()`) is answered by that logical sum too, so on
Sunrise's path the glyph model's refusal of a rounding visual sum doesn't apply. The two sums
differ only for bidirectional text whose advances round when added, which the default font's
(whole and half pixels) never do.

### Settled: the clicked slot's rules from `item_stacked_on_other`

On the **server** the part can read them: `PlayerRef::container_menu` (SDK
`host/player_menus.rs:18`), `MenuRef::kind` (`host/menus.rs:85`), `slot_backing` (`:104`),
`slot_holds` (`:112`) and `slot_gives` (`:116`). The bridge answers a click only on a plain
`Slot` over `Inventory.items` 0..36 in the inventory menu or a vanilla menu type, and only when
`slot_gives` is true and `slot_holds` equals a plain slot's `min(99, item max)` for the stacks the
click holds (so a menu part that changed the rules counts instead of being guessed). Anything
else counts.

On the **client** (prediction) it can't: those reads are `Side: server`. The prediction counts
and the client predicts vanilla until the server's answer corrects it. The smallest upstream
addition is to make `menu_slot_holds` and `menu_slot_gives` (`host/menus.rs:105-116`) `Side: both`,
answered on the predicting client from the local menu's `Slot` objects, as `menu_slot`
(`:91`) already is.
