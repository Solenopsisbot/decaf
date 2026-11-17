use crate::runtime::{MethodRef, Value, game_object};
const OWNER: &str = "net/minecraft/world/level/block/entity/BlockEntityTypes";
const DESC: &str = "Lnet/minecraft/world/level/block/entity/BlockEntityType;";
const FIELDS: &[(&str, &str)] = &[
    ("FURNACE", "furnace"),
    ("CHEST", "chest"),
    ("TRAPPED_CHEST", "trapped_chest"),
    ("ENDER_CHEST", "ender_chest"),
    ("JUKEBOX", "jukebox"),
    ("DISPENSER", "dispenser"),
    ("DROPPER", "dropper"),
    ("SIGN", "sign"),
    ("HANGING_SIGN", "hanging_sign"),
    ("MOB_SPAWNER", "mob_spawner"),
    ("CREAKING_HEART", "creaking_heart"),
    ("PISTON", "piston"),
    ("BREWING_STAND", "brewing_stand"),
    ("ENCHANTING_TABLE", "enchanting_table"),
    ("END_PORTAL", "end_portal"),
    ("BEACON", "beacon"),
    ("SKULL", "skull"),
    ("DAYLIGHT_DETECTOR", "daylight_detector"),
    ("HOPPER", "hopper"),
    ("COMPARATOR", "comparator"),
    ("BANNER", "banner"),
    ("STRUCTURE_BLOCK", "structure_block"),
    ("END_GATEWAY", "end_gateway"),
    ("COMMAND_BLOCK", "command_block"),
    ("SHULKER_BOX", "shulker_box"),
    ("CONDUIT", "conduit"),
    ("BARREL", "barrel"),
    ("SMOKER", "smoker"),
    ("BLAST_FURNACE", "blast_furnace"),
    ("LECTERN", "lectern"),
    ("BELL", "bell"),
    ("JIGSAW", "jigsaw"),
    ("CAMPFIRE", "campfire"),
    ("BEEHIVE", "beehive"),
    ("SCULK_SENSOR", "sculk_sensor"),
    ("CALIBRATED_SCULK_SENSOR", "calibrated_sculk_sensor"),
    ("SCULK_CATALYST", "sculk_catalyst"),
    ("SCULK_SHRIEKER", "sculk_shrieker"),
    ("CHISELED_BOOKSHELF", "chiseled_bookshelf"),
    ("SHELF", "shelf"),
    ("BRUSHABLE_BLOCK", "brushable_block"),
    ("DECORATED_POT", "decorated_pot"),
    ("CRAFTER", "crafter"),
    ("TRIAL_SPAWNER", "trial_spawner"),
    ("VAULT", "vault"),
    ("TEST_BLOCK", "test_block"),
    ("TEST_INSTANCE_BLOCK", "test_instance_block"),
    ("COPPER_GOLEM_STATUE", "copper_golem_statue"),
    ("POTENT_SULFUR", "potent_sulfur"),
];

pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    if r.owner != OWNER || r.descriptor != DESC {
        return None;
    }
    let id = FIELDS.iter().find(|(n, _)| *n == r.name)?.1;
    Some(game_object::client_value(format!(
        "client/block-entity-type/minecraft:{id}"
    )))
}
