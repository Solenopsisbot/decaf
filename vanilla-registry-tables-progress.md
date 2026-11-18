# Vanilla registry tables progress

## ENTITY_TYPE lookup call chain (Java 26.3, host model)

1. A jar invokes an instance method on BuiltInRegistries.ENTITY_TYPE. StdHost::invoke_instance in src/lib.rs reaches host_registry_holder::instance_call before the generic instance dispatch.
2. host_registry_holder::instance_call recognizes registry handles through host_registry::registry_id(receiver), then delegates to registry_call.
3. registry_call normalizes an Identifier or same-registry ResourceKey argument through id_argument, and handles Registry.get/getOptional returning an Optional, getValue/getValueOrThrow, getKey, and containsKey.
4. For minecraft:entity_type, the registry-specific branch calls vanilla_entity_types::handle(id). The table's ENTITY_TYPES slice is membership data. A present id returns a canonical game/entity_type/<id> GameRegistryObject handle via game_object::server_value.
5. A missing minecraft: id returns Optional.empty() for get/getOptional, and containsKey returns false. getValue uses the vanilla default minecraft:pig handle for a missing minecraft: id. A non-minecraft or otherwise unmodelled id falls through to StdHost::unknown_host_call, preserving counted unknown behavior.
6. getKey(value) is handled by the generic registry_call registration table only for observed jar registrations. The entity table does not invent reverse mappings for arbitrary values. The GameRegistryObject marker is still canonical and participates in host object identity/equality through game_object and host_ref.
7. The registry receiver itself comes from BuiltInRegistries static-field resolution in StdHost::static_field (registry_static_id -> host_registry::registry). Registries.ENTITY_TYPE resolves to the canonical ResourceKey through vanilla_registry_keys/host_ids.
8. The module is declared in lib.rs beside vanilla_blocks, vanilla_items, and vanilla_block_items; vanilla_entity_types::handle is the table-to-value boundary copied for BLOCK and ITEM.

## Implementation target

Use the same registry_call boundary for minecraft:block and minecraft:item. Return game/block/<id> and game/item/<id> handles for table members, use vanilla_block_items::item_id for block-to-item reverse data where needed, and leave modded ids and getId(value) counted unless an observed registration provides an exact answer. Verify missing minecraft: behavior with the Java 25 fixture before finalizing.
