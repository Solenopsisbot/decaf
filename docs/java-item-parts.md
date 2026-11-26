# Java item parts: a jar `Item`'s own overrides in Sunrise (design, not built)

Status: design for BUGS-3d34-round2 #2 (Runes' rune pouches act as plain items). Nothing here is
implemented yet. Bugs #3 (pouch recipes) and #4 (pouch lore) are fixed separately, in the bake and
in the runtime's `Item.Properties` model.

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
2. **Default components.** The bake writes the item's component patch for jar types too (here
   `{"bundleapi:custom_bundle_contents": {"content": [], "size_multiplier": 4}}`). That has to
   wait for step 1, because Sunrise refuses an item declaration with an unregistered component
   type, and with it the whole package.
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
   runtime. Today Sunrise reads the unregistered property as 0, so the `_filled` model never shows.
8. **The tooltip component.** The jar's `TooltipComponentMixin` maps its `TooltipComponent`
   record to its `ClientTooltipComponent`, and the factory runs `getHeight`, `getWidth` and
   `renderImage` through the client screen façade that already draws jar menu screens
   (`menu_screen.rs`).

Tests, in the repo's usual pattern: javac fixtures for each façade with JDK 25 / game 26.3
expected rows, then a real-jar test (`RUNES_JAR`) that inserts runes into a pouch through
`overrideOtherStackedOnMe` and checks the encoded component, the bar width and colour, and the
fullness property against a Fabric run.

## Upstream

None of the eight steps needs a new Sunrise API at the `3d34bbe63d` SDK, as far as these reads
show: the item-part hooks, NBT component types, served-namespace authority, item model property
registration and tooltip factories are all there. One assumption still needs checking when step 5
is built. The player inventory "has no registered type" for `menu_slot_rules`, so the part must be
able to read the clicked inventory slot's `mayPlace` and `getMaxStackSize(stack)` from the
`item_stacked_on_other` host. If it can't, the smallest addition is a read-only
`StackedOnOther::slot_rules(&ItemStack) -> { may_place, max_stack_size, may_pickup }` on the event,
answered by the slot the click resolved.

The upstream gap these bugs did show is the unread `fabric:load_conditions` key
([mod-plan.md](mod-plan.md), "Fork seams needed").
