//! Game objects for the game-class hook tier.
//!
//! A `@Inject` callback the hook tier binds runs where a host hook point fires,
//! not inside a woven Minecraft class, so the object Mixin would hand it as
//! `this` (a `ServerPlayer`, a `LivingEntity`, the `MinecraftServer`) does not
//! exist in the interpreter. This module gives that object an identity and a
//! small, honest surface:
//!
//! - **Identity.** The host describes each object it passes as a
//!   [`GameObject`]: its vanilla class, entity id, UUID, name, position and the
//!   few other facts it already knows at the hook point. The interpreter sees
//!   a game handle (#156 P16e, `game_object.rs`) keyed as the server's own
//!   objects are (`game/player/<uuid>/<session>.<life>/<id>`,
//!   `game/entity/<id>`, `game/server`), so the same entity is the same object
//!   at every firing and in every tier, and a respawned player is a new one.
//! - **Members.** A call on a marker answers from that description when the
//!   member is one this module models (`getUUID`, `getId`, `getX`,
//!   `getScoreboardName`, `level`, `getServer`...). Any other member, any
//!   `@Shadow` field of a Mixin class, and any other game-class call refuses
//!   with an error that names it. The callback stops there and the host decides
//!   what to do; nothing answers a neutral value the game never produced.
//! - **Mixin state.** Fields a Mixin class adds to its target (`shogi$cooldowns`)
//!   live in a [`GameHookState`] the host keeps per jar, keyed by the object's
//!   handle key, so a value a callback stores on a player is there at the
//!   player's next tick, and gone after a respawn (a new `ServerPlayer`). The Mixin class's no-argument constructor runs once per object,
//!   the first time a callback of that class sees it, which is where `javac`
//!   puts the field initialisers Mixin would merge into the target.
//!
//! The surface is deliberately small: it is what the bound callbacks of the
//! measured corpus reach first. Widening it is adding a row to
//! [`GameHookHost::answer`], never relaxing the refusal.

use super::*;

/// Where every game object marker starts. Markers never contain `#`, which the
/// interpreter reads as a jar object's `class#n`.
pub const GAME_OBJECT_PREFIX: &str = "game/";

/// Jar objects a hook callback builds are numbered from here, so they cannot
/// collide with the `class#n` markers an entrypoint run handed out before.
const GAME_HOOK_OBJECT_BASE: usize = 1 << 40;

/// Instance fields and arrays one jar's hook callbacks may keep alive between
/// firings. A callback that would exceed either fails instead of growing
/// without bound.
const MAX_GAME_HOOK_FIELDS: usize = 65_536;
const MAX_GAME_HOOK_ARRAYS: usize = 8_192;

/// Mixin's `@Shadow`: the member stands for one the target class declares.
const MIXIN_SHADOW_DESCRIPTOR: &str = "Lorg/spongepowered/asm/mixin/Shadow;";

/// What kind of game object a marker names; it decides the vanilla class
/// hierarchy the object answers `instanceof` for and the members it models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameObjectKind {
    /// A server player (`ServerPlayer`).
    Player,
    /// A living entity that is not a player (`LivingEntity`).
    Living,
    /// Any other entity (`Entity`).
    Entity,
    /// A dropped item (`ItemEntity`).
    ItemEntity,
    /// An item stack (`ItemStack`).
    ItemStack,
    /// A server level (`ServerLevel`).
    Level,
    /// The server (`MinecraftServer`).
    Server,
    /// A player's profile (`GameProfile`): their UUID and name.
    Profile,
    /// An item type (`Item`), keyed by its registry id.
    Item,
    /// An argument the host passes for its identity (a `DamageSource`, a
    /// `Connection`, a login cookie): every member refuses except the few the
    /// host gave a fact for (a connection's remote address, a cookie's
    /// profile).
    Opaque,
}

/// One game object a host hook point hands a callback, as the host knows it.
///
/// Every fact is optional: a member whose fact the host left out refuses
/// rather than answering a default. The constructors fill the identity; the
/// `with_*` methods add what the hook point knows.
#[derive(Debug, Clone, PartialEq)]
pub struct GameObject {
    pub kind: GameObjectKind,
    /// The most specific vanilla class the object is, as an internal name.
    pub class: String,
    /// The marker's last segment; unique per kind.
    pub key: String,
    pub entity_id: Option<i32>,
    pub uuid: Option<u128>,
    pub name: Option<String>,
    /// The entity or item type id (`minecraft:zombie`, `minecraft:diamond`).
    pub type_id: Option<String>,
    /// The level id the object is in (`minecraft:overworld`).
    pub dimension: Option<String>,
    pub position: Option<[f64; 3]>,
    pub health: Option<f32>,
    pub alive: Option<bool>,
    /// A player's game mode: 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub game_mode: Option<i32>,
    /// A stack's (or a dropped item's stack's) count.
    pub count: Option<i32>,
    /// The server: whether it is a dedicated one.
    pub dedicated: Option<bool>,
    /// A connection's remote address as `SocketAddress.toString` prints it
    /// (`/127.0.0.1:52144`).
    pub address: Option<String>,
    /// A living entity's main-hand and off-hand stacks as (item id, count);
    /// an empty hand is `("minecraft:air", 0)`, as vanilla's `ItemStack.EMPTY`.
    pub main_hand: Option<(String, i32)>,
    pub off_hand: Option<(String, i32)>,
    /// A player's session (logins) and life (respawns) on this server, as
    /// the bridge counts them (`events::FacadePlayer`): one `ServerPlayer`
    /// object each. `None` for a player the host does not count, whose key
    /// names no session (comparisons counted).
    pub incarnation: Option<(u32, u32)>,
}

impl GameObject {
    fn new(kind: GameObjectKind, class: &str, key: String) -> Self {
        Self {
            kind,
            class: class.to_owned(),
            key: key.replace('#', "_"),
            entity_id: None,
            uuid: None,
            name: None,
            type_id: None,
            dimension: None,
            position: None,
            health: None,
            alive: None,
            game_mode: None,
            count: None,
            dedicated: None,
            address: None,
            main_hand: None,
            off_hand: None,
            incarnation: None,
        }
    }

    /// A server player. Give it [`Self::with_incarnation`] and
    /// [`Self::with_entity_id`]: a respawn builds a new `ServerPlayer`, which
    /// is a new object with fresh Mixin state.
    pub fn player(uuid: u128, name: &str) -> Self {
        let mut object = Self::new(
            GameObjectKind::Player,
            "net/minecraft/server/level/ServerPlayer",
            format!("{uuid:032x}"),
        );
        object.uuid = Some(uuid);
        object.name = Some(name.to_owned());
        object.type_id = Some("minecraft:player".to_owned());
        object
    }

    /// An entity by its id; `living` picks `LivingEntity` over `Entity`.
    pub fn entity(entity_id: i32, type_id: &str, living: bool) -> Self {
        let (kind, class) = if living {
            (
                GameObjectKind::Living,
                "net/minecraft/world/entity/LivingEntity",
            )
        } else {
            (GameObjectKind::Entity, "net/minecraft/world/entity/Entity")
        };
        let mut object = Self::new(kind, class, entity_id.to_string());
        object.entity_id = Some(entity_id);
        object.type_id = Some(type_id.to_owned());
        object
    }

    /// A dropped item entity holding `count` of `item`.
    pub fn item_entity(entity_id: i32, item: &str, count: i32) -> Self {
        let mut object = Self::new(
            GameObjectKind::ItemEntity,
            "net/minecraft/world/entity/item/ItemEntity",
            entity_id.to_string(),
        );
        object.entity_id = Some(entity_id);
        object.type_id = Some(item.to_owned());
        object.count = Some(count);
        object
    }

    /// An item stack; `key` tells two stacks of one firing apart.
    pub fn item_stack(key: &str, item: &str, count: i32) -> Self {
        let mut object = Self::new(
            GameObjectKind::ItemStack,
            "net/minecraft/world/item/ItemStack",
            key.to_owned(),
        );
        object.type_id = Some(item.to_owned());
        object.count = Some(count);
        object
    }

    /// The server level of `dimension`.
    pub fn level(dimension: &str) -> Self {
        let mut object = Self::new(
            GameObjectKind::Level,
            "net/minecraft/server/level/ServerLevel",
            dimension.to_owned(),
        );
        object.dimension = Some(dimension.to_owned());
        object
    }

    /// The server.
    pub fn server(dedicated: bool) -> Self {
        let class = if dedicated {
            "net/minecraft/server/dedicated/DedicatedServer"
        } else {
            "net/minecraft/server/MinecraftServer"
        };
        let mut object = Self::new(GameObjectKind::Server, class, String::new());
        object.dedicated = Some(dedicated);
        object
    }

    /// A player's `GameProfile`.
    pub fn profile(uuid: u128, name: &str) -> Self {
        let mut object = Self::new(
            GameObjectKind::Profile,
            "com/mojang/authlib/GameProfile",
            uuid_text_of(uuid),
        );
        object.uuid = Some(uuid);
        object.name = Some(name.to_owned());
        object
    }

    /// An item type, by registry id (`minecraft:diamond_sword`).
    pub fn item(id: &str) -> Self {
        let mut object = Self::new(
            GameObjectKind::Item,
            "net/minecraft/world/item/Item",
            id.to_owned(),
        );
        object.type_id = Some(id.to_owned());
        object
    }

    /// An object passed for its identity only, of vanilla class `class`.
    pub fn opaque(class: &str, key: &str) -> Self {
        let simple = class.rsplit('/').next().unwrap_or(class);
        Self::new(
            GameObjectKind::Opaque,
            class,
            format!("{}/{key}", simple.to_ascii_lowercase()),
        )
    }

    /// A player's session and life (`events::FacadePlayer`).
    pub fn with_incarnation(mut self, session: u32, life: u32) -> Self {
        self.incarnation = Some((session, life));
        self
    }
    pub fn with_entity_id(mut self, id: i32) -> Self {
        self.entity_id = Some(id);
        self
    }
    pub fn with_uuid(mut self, uuid: u128) -> Self {
        self.uuid = Some(uuid);
        self
    }
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }
    pub fn with_dimension(mut self, dimension: &str) -> Self {
        self.dimension = Some(dimension.to_owned());
        self
    }
    pub fn with_position(mut self, position: [f64; 3]) -> Self {
        self.position = Some(position);
        self
    }
    pub fn with_health(mut self, health: f32) -> Self {
        self.health = Some(health);
        self
    }
    pub fn with_alive(mut self, alive: bool) -> Self {
        self.alive = Some(alive);
        self
    }
    pub fn with_game_mode(mut self, mode: i32) -> Self {
        self.game_mode = Some(mode);
        self
    }
    pub fn with_address(mut self, address: &str) -> Self {
        self.address = Some(address.to_owned());
        self
    }
    /// The stacks in the main and off hand, as (item id, count).
    pub fn with_hands(mut self, main: (&str, i32), off: (&str, i32)) -> Self {
        self.main_hand = Some((main.0.to_owned(), main.1));
        self.off_hand = Some((off.0.to_owned(), off.1));
        self
    }

    /// The key of this object's game handle, as the server's tiers key the
    /// same object (`game_object.rs`): a player by UUID, session, life and
    /// id; any entity by id; an item type by registry id.
    pub fn marker(&self) -> String {
        let key = &self.key;
        match self.kind {
            GameObjectKind::Player => match (self.incarnation, self.entity_id) {
                (Some((session, life)), Some(id)) => {
                    format!("{GAME_OBJECT_PREFIX}player/{key}/{session}.{life}/{id}")
                }
                (Some((session, life)), None) => {
                    format!("{GAME_OBJECT_PREFIX}player/{key}/{session}.{life}")
                }
                (None, _) => format!("{GAME_OBJECT_PREFIX}player/{key}"),
            },
            GameObjectKind::Living | GameObjectKind::Entity | GameObjectKind::ItemEntity => {
                format!("{GAME_OBJECT_PREFIX}entity/{key}")
            }
            GameObjectKind::ItemStack => format!("{GAME_OBJECT_PREFIX}stack/{key}"),
            GameObjectKind::Level => format!("{GAME_OBJECT_PREFIX}level/{key}"),
            GameObjectKind::Server => format!("{GAME_OBJECT_PREFIX}server"),
            GameObjectKind::Profile => format!("{GAME_OBJECT_PREFIX}profile/{key}"),
            GameObjectKind::Item => format!("{GAME_OBJECT_PREFIX}item/{key}"),
            GameObjectKind::Opaque => format!("{GAME_OBJECT_PREFIX}opaque/{key}"),
        }
    }

    /// The game handle a callback receives for this object (#156 P16e).
    pub fn value(&self) -> Value {
        let marker = self.marker();
        crate::runtime::game_object::value(&marker).unwrap_or(Value::Ref(Some(marker)))
    }

    /// The vanilla classes this object is an instance of, most specific
    /// first. Interfaces of vanilla classes are not listed: a callback that
    /// tests one gets `false`, which a refusal on its first call then names.
    fn hierarchy(&self) -> Vec<&str> {
        let mut classes = vec![self.class.as_str()];
        let rest: &[&str] = match self.kind {
            GameObjectKind::Player => &[
                "net/minecraft/server/level/ServerPlayer",
                "net/minecraft/world/entity/player/Player",
                "net/minecraft/world/entity/Avatar",
                "net/minecraft/world/entity/LivingEntity",
                "net/minecraft/world/entity/Entity",
            ],
            GameObjectKind::Living => &[
                "net/minecraft/world/entity/LivingEntity",
                "net/minecraft/world/entity/Entity",
            ],
            GameObjectKind::Entity => &["net/minecraft/world/entity/Entity"],
            GameObjectKind::ItemEntity => &[
                "net/minecraft/world/entity/item/ItemEntity",
                "net/minecraft/world/entity/Entity",
            ],
            GameObjectKind::ItemStack => &["net/minecraft/world/item/ItemStack"],
            GameObjectKind::Level => &[
                "net/minecraft/server/level/ServerLevel",
                "net/minecraft/world/level/Level",
            ],
            GameObjectKind::Server => &["net/minecraft/server/MinecraftServer"],
            GameObjectKind::Item => &["net/minecraft/world/item/Item"],
            GameObjectKind::Profile | GameObjectKind::Opaque => &[],
        };
        for class in rest {
            if !classes.contains(class) {
                classes.push(class);
            }
        }
        classes
    }
}

/// A change a callback asked the game to make, which the host applies after
/// the firing.
#[derive(Debug, Clone, PartialEq)]
pub enum GameHookEffect {
    /// `ServerPlayer#sendSystemMessage(Component)`: a system line in the
    /// player's chat.
    SystemMessage { player: u128, text: String },
}

/// The game objects of one firing, and what the callbacks asked of them.
///
/// The host builds one per firing: an object that is not in it (a player a
/// callback kept in a static field from an earlier firing who has since left)
/// refuses every member, because the host can no longer answer for it.
#[derive(Debug, Clone, Default)]
pub struct GameHookContext {
    objects: BTreeMap<String, GameObject>,
    effects: Vec<GameHookEffect>,
    /// Server-wide facts (world folder, game rules, registries, the channels
    /// each client takes): `facade_server.rs`. Empty unless the host set them,
    /// and then every member that needs one refuses.
    server: Arc<ServerFacts>,
}

impl GameHookContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// The server-wide facts this firing's members answer from.
    pub fn set_server_facts(&mut self, facts: Arc<ServerFacts>) {
        self.server = facts;
    }

    /// Add `object` and answer the game handle a callback receives for it.
    pub fn insert(&mut self, object: GameObject) -> Value {
        let value = object.value();
        self.objects.insert(object.marker(), object);
        value
    }

    /// The object a marker names in this firing.
    pub fn object(&self, marker: &str) -> Option<&GameObject> {
        self.objects.get(marker)
    }

    /// What the callbacks asked the game to do, in call order.
    pub fn effects(&self) -> &[GameHookEffect] {
        &self.effects
    }

    pub fn take_effects(&mut self) -> Vec<GameHookEffect> {
        std::mem::take(&mut self.effects)
    }
}

/// What one jar's hook callbacks keep between firings: the instance fields
/// and arrays they wrote on game objects (and on the jar objects those hold),
/// which objects each Mixin class has initialised, and the Mixin model of the
/// jar. A host keeps one per jar and passes it to every firing.
#[derive(Debug, Clone, Default)]
pub struct GameHookState {
    fields: BTreeMap<String, Value>,
    arrays: BTreeMap<String, crate::runtime::array_data::ArrayData>,
    next_object: usize,
    next_array: usize,
    /// `mixin class|marker` for every object a Mixin class's constructor ran on.
    initialized: BTreeSet<String>,
    model: Option<Arc<MixinModel>>,
}

impl GameHookState {
    /// How many instance fields the callbacks keep alive.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Drop everything kept on one game object that no longer exists (a
    /// player who respawned as a new object, or left): its fields, and the
    /// record that its Mixin constructors ran, so a later object with the same
    /// marker starts fresh as a new Java object would.
    pub fn forget_object(&mut self, marker: &str) {
        let prefix = format!("{marker}|");
        self.fields.retain(|key, _| !key.starts_with(&prefix));
        let suffix = format!("|{marker}");
        self.initialized.retain(|key| !key.ends_with(&suffix));
    }
}

/// How the interpreter runs a jar method a host redirects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JarCallRedirect {
    /// Ask the host, as a call to this member (the game method a Mixin
    /// `@Shadow` stands for).
    Host(MethodRef),
    /// Run this jar method instead (the Mixin class's implementation of an
    /// interface it adds to the game object).
    Jar(MethodRef),
}

/// The Mixin classes of one jar that target game classes, read once.
#[derive(Debug, Default)]
struct MixinModel {
    /// Mixin class → the game classes it targets.
    targets: BTreeMap<String, Vec<String>>,
    /// Game class → the Mixin classes that target it.
    by_target: BTreeMap<String, Vec<String>>,
    /// Mixin class → the interfaces it adds to its targets.
    interfaces: BTreeMap<String, Vec<String>>,
    /// `class|name|descriptor` of every member that stands for a game method:
    /// `@Shadow`, abstract, `@Accessor` and `@Invoker` members of a Mixin class.
    shadow_methods: BTreeSet<String>,
    /// `class|name|descriptor` of every method with a body a Mixin class
    /// declares, which a redirect may run.
    concrete_methods: BTreeSet<String>,
    /// `class#name descriptor` (the interpreter's field-key suffix) of every
    /// `@Shadow` field of a Mixin class.
    shadow_fields: BTreeSet<String>,
    /// Mixin class that extends a game class (`abstract class M extends
    /// Player`) → the fields (`name descriptor`) its own chain declares: any
    /// other field it reads or writes as `M#field` is the game superclass's.
    game_heirs: BTreeMap<String, BTreeSet<String>>,
}

/// How far a Mixin class's own superclass chain is followed.
const MAX_HEIR_CHAIN: usize = 32;

impl MixinModel {
    /// Whether the field key suffix `owner#name descriptor` names a field of
    /// the game object itself: a game class's, or one a Mixin class inherits
    /// from the game class it extends. The model holds none of them.
    fn game_field(&self, field: &str) -> bool {
        let Some((owner, member)) = field.split_once('#') else {
            return false;
        };
        is_game_class(owner)
            || self
                .game_heirs
                .get(owner)
                .is_some_and(|declared| !declared.contains(member))
    }

    fn build(classes: &BTreeMap<String, ClassFile>) -> Self {
        let mut model = Self::default();
        for (name, class) in classes {
            let Some(mixin) = class
                .annotations
                .iter()
                .find(|annotation| annotation.descriptor == MIXIN_ANNOTATION_DESCRIPTOR)
            else {
                continue;
            };
            let targets: Vec<String> = mixin_target_owners(mixin)
                .into_iter()
                .filter(|target| !classes.contains_key(target))
                .collect();
            if targets.is_empty() {
                continue;
            }
            for target in &targets {
                model
                    .by_target
                    .entry(target.clone())
                    .or_default()
                    .push(name.clone());
            }
            model.targets.insert(name.clone(), targets);
            model
                .interfaces
                .insert(name.clone(), class.interface_names());
            // The fields this class's own chain declares, if that chain
            // leaves the jar at a game class.
            let mut declared = BTreeSet::new();
            let mut current = Some(class);
            let mut exit = String::new();
            for _ in 0..MAX_HEIR_CHAIN {
                let Some(file) = current else {
                    break;
                };
                declared.extend(
                    file.fields
                        .iter()
                        .map(|field| format!("{}{}", field.name, field.descriptor)),
                );
                exit = file.super_name.clone();
                current = classes.get(&exit);
            }
            if current.is_none() && is_game_class(&exit) {
                model.game_heirs.insert(name.clone(), declared);
            }
            let (fields, methods) = member_annotations(class).unwrap_or_default();
            for (member, descriptors) in fields {
                if descriptors.iter().any(|d| d == MIXIN_SHADOW_DESCRIPTOR) {
                    let (field, descriptor) = member.split_once('|').unwrap_or((&member, ""));
                    model
                        .shadow_fields
                        .insert(format!("{name}#{field}{descriptor}"));
                }
            }
            for method in &class.methods {
                if method.name.starts_with('<') {
                    continue;
                }
                let key = format!("{}|{}", method.name, method.descriptor);
                let annotated = methods.get(&key).is_some_and(|descriptors| {
                    descriptors.iter().any(|d| {
                        d == MIXIN_SHADOW_DESCRIPTOR
                            || d == MIXIN_ACCESSOR_DESCRIPTOR
                            || d == MIXIN_INVOKER_DESCRIPTOR
                    })
                }) || method.annotations.iter().any(|a| {
                    a.descriptor == MIXIN_ACCESSOR_DESCRIPTOR
                        || a.descriptor == MIXIN_INVOKER_DESCRIPTOR
                });
                if annotated || method.access & 0x0400 != 0 {
                    model.shadow_methods.insert(format!("{name}|{key}"));
                } else if method.code.is_some() {
                    model.concrete_methods.insert(format!("{name}|{key}"));
                }
            }
        }
        model
    }

    /// The Mixin classes applied to an object of these vanilla classes.
    fn mixins_for<'a>(&'a self, hierarchy: &[&str]) -> impl Iterator<Item = &'a String> {
        let lists: Vec<&'a Vec<String>> = hierarchy
            .iter()
            .filter_map(|class| self.by_target.get(*class))
            .collect();
        lists.into_iter().flatten()
    }
}

/// Every field and method annotation descriptor of `class`, visible and
/// invisible, keyed `name|descriptor`. The parser keeps neither the invisible
/// method table nor any field annotation, and Mixin's `@Shadow` is
/// `CLASS`-retained, so the table is read back from the class bytes.
type MemberAnnotations = BTreeMap<String, Vec<String>>;

fn member_annotations(
    class: &ClassFile,
) -> Result<(MemberAnnotations, MemberAnnotations), NativeError> {
    let mut r = Reader::new(&class.raw);
    if r.u4()? != 0xCAFE_BABE {
        return Err(NativeError::InvalidClass("bad magic"));
    }
    r.skip(4)?;
    let pool = read_pool(&mut r)?;
    r.skip(6)?; // access, this, super
    let interfaces = r.u2()? as usize;
    r.skip(interfaces * 2)?;
    let fields = read_member_annotations(&mut r, &pool)?;
    let methods = read_member_annotations(&mut r, &pool)?;
    Ok((fields, methods))
}

fn read_member_annotations(
    r: &mut Reader<'_>,
    pool: &[Cp],
) -> Result<MemberAnnotations, NativeError> {
    let count = r.u2()? as usize;
    let mut out = BTreeMap::new();
    for _ in 0..count {
        r.u2()?; // access
        let name = utf8(pool, r.u2()?)?;
        let descriptor = utf8(pool, r.u2()?)?;
        let attributes = r.u2()? as usize;
        let mut descriptors = Vec::new();
        for _ in 0..attributes {
            let attribute = utf8(pool, r.u2()?)?;
            let length = r.u4()? as usize;
            let bytes = r.bytes(length)?;
            if attribute == "RuntimeInvisibleAnnotations"
                || attribute == "RuntimeVisibleAnnotations"
            {
                // A malformed table loses this member's annotations only.
                if let Ok(annotations) = read_annotations(&mut Reader::new(bytes), pool) {
                    descriptors.extend(annotations.into_iter().map(|a| a.descriptor));
                }
            }
        }
        if !descriptors.is_empty() {
            out.insert(format!("{name}|{descriptor}"), descriptors);
        }
    }
    Ok(out)
}

/// A game class the hook tier answers for itself rather than letting the
/// bootstrap host's neutral façade answer.
fn is_game_class(owner: &str) -> bool {
    owner.starts_with("net/minecraft/") || owner.starts_with("com/mojang/")
}

/// Static game-class calls that only build a value a callback then hands to a
/// modelled member (`Component.literal("hi")` for `sendSystemMessage`). The
/// bootstrap host answers them with the same markers the model reads.
/// Guava's no-argument collection factories, whose whole contract is a new,
/// empty, mutable collection: the kind the host builds for each. Callbacks
/// build their per-object state with them (`Maps.newHashMap()` in a Mixin's
/// field initialiser). A factory that takes elements or an expected size is
/// not listed and stays an unknown call.
fn empty_collection_factory(r: &MethodRef) -> Option<&'static str> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        ("com/google/common/collect/Maps", "newHashMap", "()Ljava/util/HashMap;") => {
            Some("java/util/HashMap")
        }
        ("com/google/common/collect/Maps", "newLinkedHashMap", "()Ljava/util/LinkedHashMap;") => {
            Some("java/util/LinkedHashMap")
        }
        (
            "com/google/common/collect/Maps",
            "newIdentityHashMap",
            "()Ljava/util/IdentityHashMap;",
        ) => Some("java/util/IdentityHashMap"),
        (
            "com/google/common/collect/Maps",
            "newConcurrentMap",
            "()Ljava/util/concurrent/ConcurrentMap;",
        ) => Some("java/util/concurrent/ConcurrentHashMap"),
        ("com/google/common/collect/Lists", "newArrayList", "()Ljava/util/ArrayList;") => {
            Some("java/util/ArrayList")
        }
        ("com/google/common/collect/Lists", "newLinkedList", "()Ljava/util/LinkedList;") => {
            Some("java/util/LinkedList")
        }
        ("com/google/common/collect/Sets", "newHashSet", "()Ljava/util/HashSet;") => {
            Some("java/util/HashSet")
        }
        ("com/google/common/collect/Sets", "newLinkedHashSet", "()Ljava/util/LinkedHashSet;") => {
            Some("java/util/LinkedHashSet")
        }
        ("com/google/common/collect/Sets", "newConcurrentHashSet", "()Ljava/util/Set;") => {
            Some("java/util/concurrent/ConcurrentHashMap$KeySetView")
        }
        ("com/google/common/collect/Sets", "newIdentityHashSet", "()Ljava/util/Set;") => {
            Some("java/util/Collections$SetFromMap")
        }
        _ => None,
    }
}

fn is_value_constructor(r: &MethodRef) -> bool {
    matches!(
        (r.owner.as_str(), r.name.as_str()),
        (
            "net/minecraft/network/chat/Component",
            "literal" | "translatable" | "empty" | "nullToEmpty"
        ) | (
            "net/minecraft/resources/Identifier",
            "fromNamespaceAndPath" | "withDefaultNamespace" | "parse" | "tryParse" | "tryBuild"
        ) | ("net/minecraft/resources/ResourceKey", "create")
            // A logger: the bootstrap host's slf4j model answers it.
            | ("com/mojang/logging/LogUtils", "getLogger")
    )
}

/// A game class whose instances are values the bootstrap host builds and
/// answers itself: a payload type is its channel id
/// (`new CustomPacketPayload.Type<>(id)` in a payload's static initialiser).
fn is_host_value_class(owner: &str) -> bool {
    matches!(
        owner,
        "net/minecraft/network/protocol/common/custom/CustomPacketPayload$Type"
            | "net/minecraft/world/level/storage/LevelResource"
    )
}

fn uuid_text_of(uuid: u128) -> String {
    let hex = format!("{uuid:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A connection's remote address value: the rest is its `toString`.
const SOCKET_ADDRESS: &str = "socket-address/";

/// The refusal for a member the model does not answer.
fn unmodelled(marker: &str, r: &MethodRef) -> NativeError {
    NativeError::Metadata(format!(
        "game hook object {marker} does not model {}#{}{}",
        r.owner, r.name, r.descriptor
    ))
}

/// The host a hook callback runs against: the game objects of the firing in
/// front of the jar's own bootstrap host.
struct GameHookHost<'a, H> {
    inner: &'a mut H,
    context: &'a mut GameHookContext,
    model: &'a MixinModel,
}

impl<H: Host> GameHookHost<'_, H> {
    fn object(&self, marker: &str) -> Result<GameObject, NativeError> {
        self.context.object(marker).cloned().ok_or_else(|| {
            NativeError::Metadata(format!(
                "game hook object {marker} is not live at this hook point"
            ))
        })
    }

    /// Answer one member on a game object, or refuse it.
    fn answer(
        &mut self,
        marker: &str,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        // `Object`'s `equals`, `hashCode` and `toString` are the handle
        // kind's (`game_object.rs`: an entity by its id), answered before here.
        if (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str())
            == ("java/lang/Object", "<init>", "()V")
        {
            return Ok(Value::Void);
        }
        let object = self.object(marker)?;
        let fact = |value: Option<Value>| value.ok_or_else(|| unmodelled(marker, r));
        let entity = matches!(
            object.kind,
            GameObjectKind::Player
                | GameObjectKind::Living
                | GameObjectKind::Entity
                | GameObjectKind::ItemEntity
        );
        let living = matches!(object.kind, GameObjectKind::Player | GameObjectKind::Living);
        let player = object.kind == GameObjectKind::Player;
        let string = |text: &str| Value::Ref(Some(text.to_owned()));
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("getId", "()I") if entity => fact(object.entity_id.map(Value::Int)),
            // `permissions()`: the player's level's permission set.
            ("permissions", _) if player && object.uuid.is_some() => {
                match object
                    .uuid
                    .and_then(|uuid| facade_server::player_member(&self.context.server, uuid, r))
                {
                    Some(answer) => answer.map_err(|_| unmodelled(marker, r)),
                    None => Err(unmodelled(marker, r)),
                }
            }
            ("getUUID", "()Ljava/util/UUID;") if entity => fact(
                object
                    .uuid
                    .map(|uuid| uuid_value(&uuid_text_of(uuid)))
                    .transpose()?,
            ),
            ("getStringUUID", "()Ljava/lang/String;") if entity => {
                fact(object.uuid.map(|uuid| string(&uuid_text_of(uuid))))
            }
            ("getX", "()D") if entity => fact(object.position.map(|p| Value::Float(p[0]))),
            ("getY", "()D") if entity => fact(object.position.map(|p| Value::Float(p[1]))),
            ("getZ", "()D") if entity => fact(object.position.map(|p| Value::Float(p[2]))),
            ("isAlive", "()Z") if entity => fact(object.alive.map(|a| Value::Int(i32::from(a)))),
            ("isRemoved", "()Z") if entity => fact(object.alive.map(|a| Value::Int(i32::from(!a)))),
            ("getHealth", "()F") if living => {
                fact(object.health.map(|h| Value::Float(f64::from(h))))
            }
            ("isSpectator", "()Z") if player => {
                fact(object.game_mode.map(|m| Value::Int(i32::from(m == 3))))
            }
            ("isCreative", "()Z") if player => {
                fact(object.game_mode.map(|m| Value::Int(i32::from(m == 1))))
            }
            ("getScoreboardName", "()Ljava/lang/String;") if player => {
                fact(object.name.as_deref().map(string))
            }
            ("getName" | "getDisplayName", "()Lnet/minecraft/network/chat/Component;")
                if entity =>
            {
                fact(object.name.as_deref().map(crate::runtime::host_component::literal))
            }
            ("sendSystemMessage", "(Lnet/minecraft/network/chat/Component;)V") if player => {
                let uuid = object.uuid.ok_or_else(|| unmodelled(marker, r))?;
                // The component as it is when sent (`host_component.rs`).
                let text = match args.first() {
                    Some(value) => {
                        match crate::runtime::facade_text::json_now(value, self.inner.component_states()) {
                            Some(json) => crate::runtime::facade_text::plain(&json),
                            // A component no read resolves (past the bound, a cycle).
                            None if crate::runtime::host_component::is_component(value) => {
                                return Err(unmodelled(marker, r));
                            }
                            None => crate::runtime::game_object::text(value).cloned().unwrap_or_default(),
                        }
                    }
                    None => String::new(),
                };
                self.context
                    .effects
                    .push(GameHookEffect::SystemMessage { player: uuid, text });
                Ok(Value::Void)
            }
            ("level", "()Lnet/minecraft/world/level/Level;")
            | ("level", "()Lnet/minecraft/server/level/ServerLevel;")
            | ("getLevel", "()Lnet/minecraft/server/level/ServerLevel;")
            | ("serverLevel", "()Lnet/minecraft/server/level/ServerLevel;")
                if entity =>
            {
                let dimension = object
                    .dimension
                    .as_deref()
                    .ok_or_else(|| unmodelled(marker, r))?;
                Ok(self.context.insert(GameObject::level(dimension)))
            }
            ("getServer", "()Lnet/minecraft/server/MinecraftServer;")
                if entity || object.kind == GameObjectKind::Level =>
            {
                let server = self
                    .context
                    .objects
                    .values()
                    .find(|o| o.kind == GameObjectKind::Server)
                    .map(GameObject::value)
                    .ok_or_else(|| unmodelled(marker, r))?;
                Ok(server)
            }
            ("isClientSide", "()Z") if object.kind == GameObjectKind::Level => Ok(Value::Int(0)),
            ("isDedicatedServer", "()Z") if object.kind == GameObjectKind::Server => {
                fact(object.dedicated.map(|d| Value::Int(i32::from(d))))
            }
            ("getItem", "()Lnet/minecraft/world/item/ItemStack;")
                if object.kind == GameObjectKind::ItemEntity =>
            {
                let item = object
                    .type_id
                    .clone()
                    .ok_or_else(|| unmodelled(marker, r))?;
                let count = object.count.ok_or_else(|| unmodelled(marker, r))?;
                Ok(self
                    .context
                    .insert(GameObject::item_stack(&object.key, &item, count)))
            }
            ("getCount", "()I") if object.kind == GameObjectKind::ItemStack => {
                fact(object.count.map(Value::Int))
            }
            ("isEmpty", "()Z") if object.kind == GameObjectKind::ItemStack => {
                fact(object.count.map(|c| Value::Int(i32::from(c <= 0))))
            }
            ("getItem", "()Lnet/minecraft/world/item/Item;")
                if object.kind == GameObjectKind::ItemStack =>
            {
                let item = object
                    .type_id
                    .as_deref()
                    .ok_or_else(|| unmodelled(marker, r))?;
                Ok(self.context.insert(GameObject::item(item)))
            }
            ("is", "(Lnet/minecraft/world/item/Item;)Z")
                if object.kind == GameObjectKind::ItemStack =>
            {
                let other = args
                    .first()
                    .and_then(Self::is_game_object)
                    .and_then(|other| self.context.object(other))
                    .filter(|other| other.kind == GameObjectKind::Item)
                    .and_then(|other| other.type_id.clone())
                    .ok_or_else(|| unmodelled(marker, r))?;
                Ok(Value::Int(i32::from(
                    object.type_id.as_deref() == Some(&other),
                )))
            }
            ("getMainHandItem" | "getOffhandItem", "()Lnet/minecraft/world/item/ItemStack;")
                if living =>
            {
                let (hand, stack) = if r.name == "getMainHandItem" {
                    ("mainhand", &object.main_hand)
                } else {
                    ("offhand", &object.off_hand)
                };
                let (item, count) = stack.clone().ok_or_else(|| unmodelled(marker, r))?;
                Ok(self.context.insert(GameObject::item_stack(
                    &format!("{}-{hand}", object.key),
                    &item,
                    count,
                )))
            }
            ("getGameTime" | "getDayTime", "()J") if object.kind == GameObjectKind::Level => {
                let time = if r.name == "getGameTime" {
                    self.context.server.game_time
                } else {
                    self.context.server.day_time
                };
                fact(time.map(Value::Long))
            }
            // A player's profile, and a login cookie's (the host gives the
            // cookie the joining player's UUID and name).
            ("getGameProfile", "()Lcom/mojang/authlib/GameProfile;") if player => {
                self.profile_of(marker, r, &object)
            }
            ("gameProfile", "()Lcom/mojang/authlib/GameProfile;")
                if object.kind == GameObjectKind::Opaque
                    && object.class == "net/minecraft/server/network/CommonListenerCookie" =>
            {
                self.profile_of(marker, r, &object)
            }
            ("id" | "getId", "()Ljava/util/UUID;") if object.kind == GameObjectKind::Profile => {
                fact(
                    object
                        .uuid
                        .map(|uuid| uuid_value(&uuid_text_of(uuid)))
                        .transpose()?,
                )
            }
            ("name" | "getName", "()Ljava/lang/String;")
                if object.kind == GameObjectKind::Profile =>
            {
                fact(object.name.as_deref().map(string))
            }
            ("getRemoteAddress", "()Ljava/net/SocketAddress;")
                if object.kind == GameObjectKind::Opaque
                    && object.class == "net/minecraft/network/Connection" =>
            {
                fact(
                    object
                        .address
                        .as_deref()
                        .map(|address| string(&format!("{SOCKET_ADDRESS}{address}"))),
                )
            }
            _ => {
                // Server-wide members (world path, game rules, registry
                // access) on the server, a level or an entity.
                let server_wide =
                    entity || matches!(object.kind, GameObjectKind::Server | GameObjectKind::Level);
                let args =
                    facade_server::with_level_resources(args, |m| self.inner.level_resource_id(m));
                match server_wide
                    .then(|| facade_server::server_member(&self.context.server, r, &args))
                    .flatten()
                {
                    Some(answer) => answer.map_err(|_| unmodelled(marker, r)),
                    None => Err(unmodelled(marker, r)),
                }
            }
        }
    }

    /// The `GameProfile` of an object that knows a player's UUID and name.
    fn profile_of(
        &mut self,
        marker: &str,
        r: &MethodRef,
        object: &GameObject,
    ) -> Result<Value, NativeError> {
        match (object.uuid, object.name.as_deref()) {
            (Some(uuid), Some(name)) => Ok(self.context.insert(GameObject::profile(uuid, name))),
            _ => Err(unmodelled(marker, r)),
        }
    }

    /// A call on a value marker the hook tier hands out itself: a server-wide
    /// marker (`facade_server.rs`) or a connection's remote address.
    fn answer_value(
        &mut self,
        text: &str,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if facade_server::owns(text) {
            // A registry's `keySet()`: a set of its own (#156 P14d2).
            if let Some(ids) = facade_server::key_set(&self.context.server, text, r) {
                return Some(ids.and_then(|ids| {
                    self.inner
                        .factory_collection("java/util/Collections", "unmodifiableSet", "set", ids)
                        .ok_or_else(|| NativeError::UnknownHostCall(r.clone()))
                }));
            }
            return Some(
                // A Java exception vanilla throws there stays one.
                facade_server::answer(&self.context.server, text, r, args).map_err(|error| {
                    if matches!(error, NativeError::UncaughtException { .. }) {
                        return error;
                    }
                    NativeError::Metadata(format!(
                        "game hook value {text} does not model {}#{}{}",
                        r.owner, r.name, r.descriptor
                    ))
                }),
            );
        }
        let address = text.strip_prefix(SOCKET_ADDRESS)?;
        Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("toString", "()Ljava/lang/String;") => Ok(Value::Ref(Some(address.to_owned()))),
            ("hashCode", "()I") => Ok(Value::Int(stable_hash(address))),
            ("equals", "(Ljava/lang/Object;)Z") => Ok(Value::Int(i32::from(
                args.first()
                    .and_then(crate::runtime::game_object::text)
                    .is_some_and(|o| o == text),
            ))),
            _ => Err(NativeError::Metadata(format!(
                "game hook value {text} does not model {}#{}{}",
                r.owner, r.name, r.descriptor
            ))),
        })
    }

    fn is_game_object(value: &Value) -> Option<&str> {
        crate::runtime::game_object::text(value)
            .map(String::as_str)
            .filter(|marker| marker.starts_with(GAME_OBJECT_PREFIX))
    }
}

/// Whether `value` is what `Event.invoker()` answers for a Fabric event
/// (`host_events.rs`).
fn is_fabric_event_invoker(value: &Value) -> bool {
    crate::runtime::host_events::key_of(value, crate::runtime::HostKind::FabricInvoker).is_some()
}

impl<H: Host> Host for GameHookHost<'_, H> {
    // Every member the bootstrap host answers is forwarded to it, so a callback's collections,
    // hash tables, iterators, console and counted operands behave as an entrypoint's do (#246's
    // review: the trait defaults answered silently here, a respawned player's two lives were two
    // `HashSet` entries, a player printed as `null`). The members below these forwards are this
    // tier's own: game objects, `@Shadow` members and the calls only the hook point answers.
    fn neoforge_attachments(
        &mut self,
    ) -> Option<(
        &mut neoforge_attachments::AttachmentState,
        &mut BTreeMap<String, usize>,
    )> {
        self.inner.neoforge_attachments()
    }

    fn clone_original_of(&self, text: &str) -> Option<u128> {
        self.inner.clone_original_of(text)
    }

    fn immutable_collection_from(
        &mut self,
        owner: &str,
        name: &str,
        kind: &str,
        values: Vec<Value>,
    ) -> Option<Value> {
        self.inner
            .immutable_collection_from(owner, name, kind, values)
    }

    fn neoforge_bus(
        &mut self,
    ) -> Option<(&mut neoforge_bus::BusState, &mut BTreeMap<String, usize>)> {
        self.inner.neoforge_bus()
    }

    fn neoforge_imc(
        &mut self,
    ) -> Option<(&mut neoforge_imc::ImcHandle, &mut BTreeMap<String, usize>)> {
        self.inner.neoforge_imc()
    }

    fn unknown_operand_call(&mut self, r: &MethodRef) -> Result<Value, NativeError> {
        self.inner.unknown_operand_call(r)
    }

    fn modelled_to_string(&mut self, value: &Value) -> Option<String> {
        self.inner.modelled_to_string(value)
    }

    fn modelled_to_string_checked(&mut self, value: &Value) -> Result<Option<String>, NativeError> {
        self.inner.modelled_to_string_checked(value)
    }

    fn mutable_collection_entries(&self, value: &Value) -> Option<Vec<Value>> {
        self.inner.mutable_collection_entries(value)
    }

    fn replace_mutable_collection(&mut self, value: &Value, values: Vec<Value>) {
        self.inner.replace_mutable_collection(value, values)
    }

    fn unmodifiable_view_source(&self, marker: &str) -> Option<String> {
        self.inner.unmodifiable_view_source(marker)
    }

    fn unmodifiable_list_view_as(&mut self, class: &str, source: &Value) -> Option<Value> {
        self.inner.unmodifiable_list_view_as(class, source)
    }

    fn has_mutable_collection(&self, marker: &str) -> bool {
        self.inner.has_mutable_collection(marker)
    }

    fn collection_generation(&self, marker: &str) -> Option<u64> {
        self.inner.collection_generation(marker)
    }

    fn collection_slice(&self, marker: &str) -> Option<&[Value]> {
        self.inner.collection_slice(marker)
    }

    fn collection_vec_mut(&mut self, marker: &str) -> Option<&mut Vec<Value>> {
        self.inner.collection_vec_mut(marker)
    }

    fn take_hash_table(&mut self, marker: &str) -> Option<hash_table::HashTable> {
        self.inner.take_hash_table(marker)
    }

    fn store_hash_table(&mut self, marker: &str, table: hash_table::HashTable) {
        self.inner.store_hash_table(marker, table)
    }

    fn collection_comparator(&self, marker: &str) -> Option<Value> {
        self.inner.collection_comparator(marker)
    }

    fn set_collection_comparator(&mut self, marker: &str, comparator: Value) {
        self.inner.set_collection_comparator(marker, comparator)
    }

    fn take_red_black_tree(&mut self, marker: &str) -> Option<tree_map::RedBlackTree> {
        self.inner.take_red_black_tree(marker)
    }

    fn store_red_black_tree(&mut self, marker: &str, tree: tree_map::RedBlackTree) {
        self.inner.store_red_black_tree(marker, tree)
    }

    fn take_probe_table(&mut self, marker: &str) -> Option<immutable_collections::ProbeTable> {
        self.inner.take_probe_table(marker)
    }

    fn store_probe_table(&mut self, marker: &str, table: immutable_collections::ProbeTable) {
        self.inner.store_probe_table(marker, table)
    }

    fn note_view_copy(&mut self, marker: &str, source: &str) {
        self.inner.note_view_copy(marker, source)
    }

    fn is_view_copy(&self, marker: &str) -> bool {
        self.inner.is_view_copy(marker)
    }

    fn iterator_guard(&self, marker: &str) -> Option<iteration_guard::IteratorGuard> {
        self.inner.iterator_guard(marker)
    }

    fn store_iterator_guard(&mut self, marker: &str, guard: iteration_guard::IteratorGuard) {
        self.inner.store_iterator_guard(marker, guard)
    }

    fn iterator_cursor(&self, marker: &str) -> Option<usize> {
        self.inner.iterator_cursor(marker)
    }

    fn view_copy_source(&self, marker: &str) -> Option<String> {
        self.inner.view_copy_source(marker)
    }

    fn note_view_kind(&mut self, marker: &str, kind: &str) {
        self.inner.note_view_kind(marker, kind)
    }

    fn view_copy_kind(&self, marker: &str) -> Option<String> {
        self.inner.view_copy_kind(marker)
    }

    fn view_copies_of(&self, source: &str) -> Vec<String> {
        self.inner.view_copies_of(source)
    }

    fn synchronized_wrapped(&self, value: &Value) -> Option<Value> {
        self.inner.synchronized_wrapped(value)
    }

    fn mod_stamp(&self, marker: &str) -> mod_count::ModStamp {
        self.inner.mod_stamp(marker)
    }

    fn store_mod_stamp(&mut self, marker: &str, stamp: mod_count::ModStamp) {
        self.inner.store_mod_stamp(marker, stamp)
    }

    fn sub_list_link(&self, marker: &str) -> Option<mod_count::SubListLink> {
        self.inner.sub_list_link(marker)
    }

    fn store_sub_list_link(&mut self, marker: &str, link: Option<mod_count::SubListLink>) {
        self.inner.store_sub_list_link(marker, link)
    }

    fn sub_lists_of(&self, parent: &str) -> Vec<(String, mod_count::SubListLink)> {
        self.inner.sub_lists_of(parent)
    }

    fn new_list_iterator(&mut self, state: host_list_iterator::ListIter) -> Option<String> {
        self.inner.new_list_iterator(state)
    }

    fn list_iterator(&self, marker: &str) -> Option<host_list_iterator::ListIter> {
        self.inner.list_iterator(marker)
    }

    fn store_list_iterator(&mut self, marker: &str, state: host_list_iterator::ListIter) {
        self.inner.store_list_iterator(marker, state)
    }

    fn sub_lists_of_any(&self) -> usize {
        self.inner.sub_lists_of_any()
    }

    fn deque_layout(&self, marker: &str) -> Option<array_deque::DequeLayout> {
        self.inner.deque_layout(marker)
    }

    fn store_deque_layout(&mut self, marker: &str, layout: Option<array_deque::DequeLayout>) {
        self.inner.store_deque_layout(marker, layout)
    }

    fn host_iterator(&self, marker: &str) -> Option<(usize, usize, Option<String>)> {
        self.inner.host_iterator(marker)
    }

    fn host_iterator_next(&mut self, marker: &str) -> Result<Value, NativeError> {
        self.inner.host_iterator_next(marker)
    }

    fn set_host_iterator_cursor(&mut self, marker: &str, cursor: usize) {
        self.inner.set_host_iterator_cursor(marker, cursor)
    }

    fn write_console(&mut self, text: &str) {
        self.inner.write_console(text)
    }

    fn resource_stream_bytes(&mut self, marker: &str, limit: Option<usize>) -> Option<Vec<u8>> {
        self.inner.resource_stream_bytes(marker, limit)
    }

    fn note_content_builder(&mut self, event: ContentBuilderEvent) {
        self.inner.note_content_builder(event)
    }

    fn platform_class_present(&self, name: &str) -> bool {
        self.inner.platform_class_present(name)
    }

    fn unmodifiable_list(&mut self, values: Vec<Value>) -> Option<Value> {
        self.inner.unmodifiable_list(values)
    }

    fn factory_collection(
        &mut self,
        owner: &str,
        name: &str,
        kind: &str,
        values: Vec<Value>,
    ) -> Option<Value> {
        self.inner.factory_collection(owner, name, kind, values)
    }

    // A `@Shadow` field is the game object's own: refused (`get_static`); any other instance
    // field is the bootstrap host's.
    fn get_field(&mut self, receiver: &Value, r: &MethodRef) -> Result<Value, NativeError> {
        if self
            .model
            .shadow_fields
            .contains(&format!("{}#{}{}", r.owner, r.name, r.descriptor))
        {
            return self.get_static(r);
        }
        // A game class's own field read on a game object (`player.connection`): the model
        // holds none, so the read refuses as a `@Shadow` one does, never the bootstrap host's
        // `null` (#248's review: cpm's `ServerPlayer.connection` read answered null, and the
        // NPE that followed looked like cpm's own).
        if Self::is_game_object(receiver).is_some()
            && self
                .model
                .game_field(&format!("{}#{}{}", r.owner, r.name, r.descriptor))
        {
            return Err(NativeError::Metadata(format!(
                "game hook callback reads the game's field {}#{} on a game object, which is not modelled",
                r.owner, r.name
            )));
        }
        self.inner.get_field(receiver, r)
    }

    /// What the firing describes `key` as (its vanilla hierarchy and the interfaces the jar's
    /// Mixins add to it): `Some(true)` where the description names `wanted`, never a proof of
    /// `false` (a description lists no vanilla interface), so anything else is counted.
    fn described_instance_of(&self, key: &str, wanted: &str) -> Option<bool> {
        self.context.object(key)?;
        self.host_instance_of(key, wanted).then_some(true)
    }

    fn empty_iterator(&mut self) -> Value {
        self.inner.empty_iterator()
    }

    fn empty_enumeration(&mut self) -> Value {
        self.inner.empty_enumeration()
    }

    fn new_enumeration(&mut self, over: crate::runtime::host_iterator::EnumerationOver) -> Option<Value> {
        self.inner.new_enumeration(over)
    }

    fn enumeration_over(&self, marker: &str) -> Option<crate::runtime::host_iterator::EnumerationOver> {
        self.inner.enumeration_over(marker)
    }

    fn store_enumeration(&mut self, marker: &str, over: crate::runtime::host_iterator::EnumerationOver) {
        self.inner.store_enumeration(marker, over);
    }

    fn constructed_uuid(&self, value: &Value) -> Option<(i64, i64)> {
        self.inner.constructed_uuid(value)
    }

    fn map_entry_parts(&self, value: &Value) -> Option<(Value, Value)> {
        self.inner.map_entry_parts(value)
    }

    fn mint_map_entry(
        &mut self,
        map: Option<&Value>,
        key: &Value,
        value: &Value,
        made: crate::runtime::host_map_entry::EntryMade,
    ) -> Value {
        self.inner.mint_map_entry(map, key, value, made)
    }

    fn mint_map_entries(
        &mut self,
        map: &Value,
        pairs: &[Value],
        made: crate::runtime::host_map_entry::EntryMade,
    ) -> Vec<Value> {
        self.inner.mint_map_entries(map, pairs, made)
    }

    fn unwrapped_view_entry(&self, entry: &Value) -> Option<Value> {
        self.inner.unwrapped_view_entry(entry)
    }

    fn entries_before_call(&mut self, marker: &str, name: &str, args: &[Value]) {
        self.inner.entries_before_call(marker, name, args);
    }

    fn entries_after_call(&mut self, marker: &str) {
        self.inner.entries_after_call(marker);
    }

    fn component_states(&self) -> Option<&crate::runtime::host_component::States> {
        self.inner.component_states()
    }

    fn component_states_mut(&mut self) -> Option<&mut crate::runtime::host_component::States> {
        self.inner.component_states_mut()
    }

    fn feedback_calls_supplier(&self, source: &Value, broadcast: bool) -> Option<Option<bool>> {
        self.inner.feedback_calls_supplier(source, broadcast)
    }

    fn send_feedback(
        &mut self,
        r: &MethodRef,
        component: &Value,
        broadcast: bool,
    ) -> Result<Value, NativeError> {
        self.inner.send_feedback(r, component, broadcast)
    }

    fn note_entrypoint_constructed(&mut self, kind: &str, class: &str, instance: &Value) {
        self.inner
            .note_entrypoint_constructed(kind, class, instance);
    }

    fn fabric_entrypoints(&self, kind: &str) -> Vec<(String, String)> {
        self.inner.fabric_entrypoints(kind)
    }

    fn fabric_entrypoint_object(
        &self,
        kind: &str,
        class: &str,
        requested: &str,
        ordinal: usize,
    ) -> Option<Value> {
        self.inner
            .fabric_entrypoint_object(kind, class, requested, ordinal)
    }

    fn get_static(&mut self, r: &MethodRef) -> Result<Value, NativeError> {
        // An instance-field read the callbacks never wrote reaches the host
        // too. A Mixin `@Shadow` field is the game object's own field, which
        // the model does not hold: refuse instead of answering 0/null.
        if self
            .model
            .shadow_fields
            .contains(&format!("{}#{}{}", r.owner, r.name, r.descriptor))
        {
            return Err(NativeError::Metadata(format!(
                "game hook callback reads the game's field {}#{} through @Shadow, which is not modelled",
                r.owner, r.name
            )));
        }
        self.inner.get_static(r)
    }

    fn invoke(&mut self, r: &MethodRef, args: &[Value]) -> Result<Value, NativeError> {
        if let Some(result) = guava_model::sized_factory_class(r, args) {
            let class = result?;
            return self.inner.empty_mutable_collection(class).ok_or_else(|| {
                NativeError::Metadata(format!(
                    "game hook callback calls {}#{}{}, and the host models no collection",
                    r.owner, r.name, r.descriptor
                ))
            });
        }
        if let Some(class) = empty_collection_factory(r) {
            return self.inner.empty_mutable_collection(class).ok_or_else(|| {
                NativeError::Metadata(format!(
                    "game hook callback calls {}#{}{}, and the host models no collection",
                    r.owner, r.name, r.descriptor
                ))
            });
        }
        // `ServerPlayNetworking.canSend(player, type)`: whether that player's
        // client takes the channel, from the server facts.
        let player = args
            .first()
            .and_then(Self::is_game_object)
            .and_then(|marker| self.context.object(marker))
            .filter(|object| object.kind == GameObjectKind::Player)
            .and_then(|object| object.uuid);
        let channel = args.get(1).and_then(|value| {
            facade_server::channel_of(value, |marker| self.inner.payload_type_id(marker))
        });
        if let Some(answer) =
            facade_server::can_send(&self.context.server, r, player, channel.as_deref())
        {
            return answer.map_err(|_| {
                NativeError::Metadata(format!(
                    "game hook callback calls {}#{}{} for a player or channel the firing does not name",
                    r.owner, r.name, r.descriptor
                ))
            });
        }
        // A cast or `instanceof` the description cannot settle is counted by the bootstrap
        // host, never a refusal that ends the firing (#246's review).
        if is_game_class(&r.owner) && matches!(r.name.as_str(), "<checkcast>" | "<instanceof>") {
            return self.inner.invoke(r, args);
        }
        if is_game_class(&r.owner) && !is_value_constructor(r) {
            return Err(NativeError::Metadata(format!(
                "game hook callback calls {}#{}{}, which the hook tier does not model",
                r.owner, r.name, r.descriptor
            )));
        }
        self.inner.invoke(r, args)
    }

    fn invoke_instance(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        // The server's saved data (storage, types, a `SavedData`'s dirty
        // flag) lives on the jar's bootstrap host, across firings.
        if facade_saved_data::owns_call(r, crate::runtime::game_object::text(receiver).map(String::as_str)) {
            return self.inner.invoke_instance(r, receiver, args);
        }
        // A Fabric `Event`'s invoker is a host marker, not the jar's own
        // array-backed invoker over its listeners, so any answer here would be
        // made up (an `Allow` invoker answering `false` cancels what vanilla
        // keeps). The firing fails instead.
        if is_fabric_event_invoker(receiver) {
            return Err(NativeError::Metadata(format!(
                "game hook callback calls {}#{}{} on a Fabric event's invoker, which the hook tier does not run",
                r.owner, r.name, r.descriptor
            )));
        }
        if let Some(text) = crate::runtime::game_object::text(receiver).cloned()
            && let Some(answer) = self.answer_value(&text, r, args)
        {
            return answer;
        }
        // `BuiltInRegistries.ITEM.getKey(item)` for an item the firing names.
        if crate::runtime::host_registry::registry_id(receiver) == Some("minecraft:item")
            && r.name == "getKey"
            && r.descriptor == "(Ljava/lang/Object;)Lnet/minecraft/resources/Identifier;"
            && let Some(id) = args
                .first()
                .and_then(Self::is_game_object)
                .and_then(|item| self.context.object(item))
                .filter(|item| item.kind == GameObjectKind::Item)
                .and_then(|item| item.type_id.clone())
        {
            return Ok(identifier_parse(&id));
        }
        if let Some(marker) = Self::is_game_object(receiver) {
            let marker = marker.to_owned();
            return self.answer(&marker, r, args);
        }
        // A host handle (an `Identifier`, a `ResourceKey`) is a value the
        // bootstrap host models (`host_ids.rs`).
        if is_game_class(&r.owner)
            && !is_host_value_class(&r.owner)
            && !matches!(receiver, Value::Host(_))
        {
            return Err(NativeError::Metadata(format!(
                "game hook callback calls {}#{}{} on {}, which the hook tier does not model",
                r.owner,
                r.name,
                r.descriptor,
                crate::runtime::game_object::text(receiver).map_or("null", String::as_str)
            )));
        }
        self.inner.invoke_instance(r, receiver, args)
    }

    fn mutable_collection_values(&self, value: &Value) -> Option<Vec<Value>> {
        self.inner.mutable_collection_values(value)
    }

    fn holds_collection(&self, value: &Value) -> bool {
        self.inner.holds_collection(value)
    }

    fn collection_len(&self, value: &Value) -> Option<usize> {
        self.inner.collection_len(value)
    }

    fn is_jdk_list_of(&self, marker: &str) -> bool {
        self.inner.is_jdk_list_of(marker)
    }

    fn service_providers(&self, service: &str) -> Vec<String> {
        self.inner.service_providers(service)
    }

    fn resource_stream_text(&mut self, marker: &str) -> Option<String> {
        self.inner.resource_stream_text(marker)
    }

    fn linked_class_table(&self) -> Option<&Arc<LinkedClassTable>> {
        self.inner.linked_class_table()
    }

    fn models_class(&self, name: &str) -> bool {
        self.inner.models_class(name)
    }

    fn throwable_message(&self, marker: &str) -> Option<Option<String>> {
        self.inner.throwable_message(marker)
    }

    fn record_throwable(&mut self, marker: &str, message: Option<String>) {
        self.inner.record_throwable(marker, message);
    }

    fn throwable_suppressed(&self, marker: &str) -> Option<Vec<Value>> {
        self.inner.throwable_suppressed(marker)
    }

    fn record_throwable_suppressed(&mut self, marker: &str, value: Value) {
        self.inner.record_throwable_suppressed(marker, value);
    }

    fn record_throwable_cause(&mut self, marker: &str, cause: Value) {
        self.inner.record_throwable_cause(marker, cause);
    }

    fn record_wrapped_cause(&mut self, class: &str, message: Option<&str>, cause: Value) {
        self.inner.record_wrapped_cause(class, message, cause);
    }

    fn take_wrapped_cause(&mut self, class: &str, message: &Option<String>) -> Option<Value> {
        self.inner.take_wrapped_cause(class, message)
    }

    fn record_wrapped_suppressed(&mut self, class: &str, message: Option<&str>, value: Value) {
        self.inner.record_wrapped_suppressed(class, message, value);
    }

    fn take_wrapped_suppressed(&mut self, class: &str, message: &Option<String>) -> Vec<Value> {
        self.inner.take_wrapped_suppressed(class, message)
    }

    fn record_fabric_entrypoint_failure(&mut self, kind: &str, class: &str, cause: Value) {
        self.inner
            .record_fabric_entrypoint_failure(kind, class, cause);
    }

    fn take_fabric_entrypoint_failure(&mut self, kind: &str, class: &str) -> Option<Value> {
        self.inner.take_fabric_entrypoint_failure(kind, class)
    }

    fn persistent_class_state(&self) -> (BTreeMap<String, Value>, BTreeSet<String>) {
        self.inner.persistent_class_state()
    }

    fn store_persistent_class_state(
        &mut self,
        fields: BTreeMap<String, Value>,
        initialized: BTreeSet<String>,
    ) -> Result<(), NativeError> {
        self.inner.store_persistent_class_state(fields, initialized)
    }

    fn persistent_io_state(
        &self,
    ) -> (
        BTreeMap<String, String>,
        BTreeMap<String, String>,
        BTreeMap<String, Vec<u8>>,
        BTreeSet<String>,
    ) {
        self.inner.persistent_io_state()
    }

    fn store_persistent_io_state(
        &mut self,
        paths: BTreeMap<String, String>,
        files: BTreeMap<String, String>,
        file_bytes: BTreeMap<String, Vec<u8>>,
        dirs: BTreeSet<String>,
    ) -> Result<(), NativeError> {
        self.inner
            .store_persistent_io_state(paths, files, file_bytes, dirs)
    }

    fn take_persistent_heap(&mut self) -> Option<JavaHeap> {
        self.inner.take_persistent_heap()
    }

    fn store_persistent_heap(&mut self, heap: JavaHeap) -> Result<(), NativeError> {
        self.inner.store_persistent_heap(heap)
    }

    fn object_collected(&self, key: &str) -> bool {
        self.inner.object_collected(key)
    }

    fn pause_collections(&mut self, paused: bool) {
        self.inner.pause_collections(paused);
    }

    fn empty_mutable_collection(&mut self, kind: &str) -> Option<Value> {
        self.inner.empty_mutable_collection(kind)
    }

    fn make_collection(&mut self, class: &str, values: Vec<Value>) -> Option<Value> {
        self.inner.make_collection(class, values)
    }

    fn owned_collection(
        &mut self,
        of: &Value,
        member: &str,
        class: &str,
        follow: bool,
        values: Vec<Value>,
    ) -> Option<Value> {
        self.inner
            .owned_collection(of, member, class, follow, values)
    }

    fn array_view(
        &mut self,
        array: &str,
        offset: usize,
        values: Vec<Value>,
        window: bool,
    ) -> Option<Value> {
        self.inner.array_view(array, offset, values, window)
    }

    fn array_view_of(&self, marker: &str) -> Option<(String, usize, usize, bool)> {
        self.inner.array_view_of(marker)
    }

    fn has_array_views(&self, array: &str) -> bool {
        self.inner.has_array_views(array)
    }

    fn array_written(&mut self, array: &str, values: &[Value]) {
        self.inner.array_written(array, values);
    }

    fn changed_array_views(&mut self) -> Vec<(String, usize, Vec<Value>)> {
        self.inner.changed_array_views()
    }

    fn counted_unknown_calls(&self) -> usize {
        self.inner.counted_unknown_calls()
    }

    fn jar_module_descriptor(&self) -> Option<bool> {
        self.inner.jar_module_descriptor()
    }

    fn payload_type_id(&self, marker: &str) -> Option<String> {
        self.inner.payload_type_id(marker)
    }

    fn level_resource_id(&self, marker: &str) -> Option<String> {
        self.inner.level_resource_id(marker)
    }

    fn redirect_jar_call(
        &self,
        class: &str,
        method: &Method,
        receiver: Option<&Value>,
    ) -> Option<JarCallRedirect> {
        let shadow = self.model.targets.get(class).and_then(|targets| {
            self.model
                .shadow_methods
                .contains(&format!("{class}|{}|{}", method.name, method.descriptor))
                .then(|| targets.first().cloned())
                .flatten()
        });
        if let Some(target) = shadow {
            return Some(JarCallRedirect::Host(MethodRef {
                owner: target,
                name: method.name.clone(),
                descriptor: method.descriptor.clone(),
            }));
        }
        if method.code.is_some() {
            return None;
        }
        // An abstract member on a game object: a Mixin class applied to the
        // object may implement it (an interface the Mixin adds). Otherwise the
        // host is asked, which refuses it, instead of the interpreter's
        // neutral answer for a body it cannot find.
        let marker = receiver.and_then(Self::is_game_object)?;
        let object = self.context.object(marker)?;
        let hierarchy = object.hierarchy();
        for mixin in self.model.mixins_for(&hierarchy) {
            let implements = self
                .model
                .interfaces
                .get(mixin)
                .is_some_and(|interfaces| interfaces.iter().any(|i| i == class));
            if implements
                && self
                    .model
                    .concrete_methods
                    .contains(&format!("{mixin}|{}|{}", method.name, method.descriptor))
            {
                return Some(JarCallRedirect::Jar(MethodRef {
                    owner: mixin.clone(),
                    name: method.name.clone(),
                    descriptor: method.descriptor.clone(),
                }));
            }
        }
        Some(JarCallRedirect::Host(MethodRef {
            owner: class.to_owned(),
            name: method.name.clone(),
            descriptor: method.descriptor.clone(),
        }))
    }

    fn host_instance_of(&self, marker: &str, wanted: &str) -> bool {
        let Some(object) = self.context.object(marker) else {
            return false;
        };
        let hierarchy = object.hierarchy();
        if hierarchy.contains(&wanted) {
            return true;
        }
        self.model.mixins_for(&hierarchy).any(|mixin| {
            mixin == wanted
                || self
                    .model
                    .interfaces
                    .get(mixin)
                    .is_some_and(|interfaces| interfaces.iter().any(|i| i == wanted))
        })
    }
}

impl NativeJar {
    /// Run the callbacks a jar registered at one host hook point, against the
    /// game objects of this firing and the state earlier firings left.
    ///
    /// This is [`Self::run_game_hook`] for a host that fires the point from
    /// the game: `context` holds the objects the receiver and the arguments
    /// name (see [`GameHookContext::insert`]), `state` is the jar's own and
    /// persists between firings, and `budget` bounds the firing's wall-clock
    /// time. A call the model does not answer fails the firing with an error
    /// naming it; the callbacks that ran before it keep their effects, and the
    /// host decides whether to fire the jar again.
    ///
    /// On a resident host (`StdHost`) the callbacks run on the jar's own
    /// objects (`heap.rs`): what its entrypoint built and earlier firings
    /// changed, through statics or lambda captures. Their writes to those
    /// objects and to statics stay even when the firing fails, as a JVM's
    /// would; only the fields a failed firing wrote on game objects are
    /// dropped. Fields on game objects always stay in `state`, so
    /// [`GameHookState::forget_object`] still forgets them per object.
    #[allow(clippy::too_many_arguments)]
    pub fn run_game_hook_in<H: Host>(
        &self,
        registrations: &GameHookRegistrations,
        state: &mut GameHookState,
        context: &mut GameHookContext,
        hook: &str,
        receiver: Option<Value>,
        args: &[Value],
        host: &mut H,
        budget: &ExecutionBudget,
    ) -> Result<GameHookRun, NativeError> {
        self.run_game_hook_returning(
            registrations,
            state,
            context,
            hook,
            receiver,
            args,
            None,
            host,
            budget,
        )
    }

    /// [`Self::run_game_hook_in`] for a site whose vanilla method returns a
    /// value (`hook_shapes.rs`).
    ///
    /// `actual` is the method's own result at a `RETURN`/`TAIL` site, which
    /// the callbacks read with `CallbackInfoReturnable.getReturnValue`; every
    /// other site passes `None`, and a mismatch is an error before any
    /// callback runs. [`GameHookRun::returned`] is what the host must make the
    /// method return: at a `HEAD` site, set exactly when a cancellable
    /// callback cancelled (its `setReturnValue` value, else the type's
    /// default), so the host skips its own work; at a `RETURN`/`TAIL` site,
    /// a cancellable callback's override of `actual`.
    ///
    /// Mixin's rule for a callback that is not `cancellable` holds here too:
    /// its `cancel()` or `setReturnValue()` throws `CancellationException`, so
    /// the firing fails with that error rather than dropping the call.
    #[allow(clippy::too_many_arguments)]
    pub fn run_game_hook_returning<H: Host>(
        &self,
        registrations: &GameHookRegistrations,
        state: &mut GameHookState,
        context: &mut GameHookContext,
        hook: &str,
        receiver: Option<Value>,
        args: &[Value],
        actual: Option<Value>,
        host: &mut H,
        budget: &ExecutionBudget,
    ) -> Result<GameHookRun, NativeError> {
        let entries = registrations.entries(hook);
        let Some(first) = entries.first() else {
            return Err(NativeError::Metadata(format!(
                "no native game hook callback is registered at {hook}"
            )));
        };
        let first_site = game_hook_site_named(&first.hook, first.version).ok_or_else(|| {
            NativeError::Metadata(format!(
                "game hook registration names {} version {}, which this runtime does not know",
                first.hook, first.version
            ))
        })?;
        hook_shapes::check_actual_result(first_site, actual.as_ref())?;
        let expected = arg_count(&first.target.descriptor);
        if args.len() != expected {
            return Err(NativeError::Metadata(format!(
                "game hook point {hook} passes {} argument(s) to {}#{}{}, got {}",
                expected,
                first.target.owner,
                first.target.name,
                first.target.descriptor,
                args.len()
            )));
        }
        let mut checked = Vec::with_capacity(entries.len());
        for entry in entries {
            let site = game_hook_site_named(&entry.hook, entry.version).ok_or_else(|| {
                NativeError::Metadata(format!(
                    "game hook registration names {} version {}, which this runtime does not know",
                    entry.hook, entry.version
                ))
            })?;
            match entry.shape {
                GameHookShape::Inject => game_hook_callback(&self.classes, &entry.callback, site),
                GameHookShape::ModifyReturnValue => {
                    mixin_extras::modify_return_callback(&self.classes, &entry.callback, site)
                        .map(|_| ())
                }
            }
            .map_err(|reason| game_hook_error(&reason))?;
            checked.push(self.game_hook_callback_values(entry, args)?);
        }
        let model = Arc::clone(
            state
                .model
                .get_or_insert_with(|| Arc::new(MixinModel::build(&self.classes))),
        );
        let (persistent_static_fields, persistent_initialized_classes) =
            host.persistent_class_state();
        let (io_paths, io_files, io_file_bytes, io_dirs) = host.persistent_io_state();
        let mut wrapped = GameHookHost {
            inner: host,
            context,
            model: &model,
        };
        let linked = wrapped.linked_class_table().cloned().unwrap_or_default();
        let mut callback_infos = CallbackInfos::new();
        let mut results = Vec::with_capacity(entries.len());
        let mut cancelled = false;
        let mut initialized = state.initialized.clone();
        let mut runtime = Runtime {
            classes: &self.classes,
            mixin_index: std::cell::OnceCell::new(),
            linked,
            host: &mut wrapped,
            calls: 0,
            instructions: 0,
            fields: state.fields.clone(),
            static_fields: persistent_static_fields,
            initialized_classes: persistent_initialized_classes,
            arrays: state.arrays.clone(),
            next_array: state.next_array.max(GAME_HOOK_OBJECT_BASE),
            next_object: state.next_object.max(GAME_HOOK_OBJECT_BASE),
            streams: BTreeMap::new(),
            next_stream: 0,
            pipelines: Default::default(),
            enum_constants: BTreeMap::new(),
            string_builders: BTreeMap::new(),
            gson_writers: BTreeMap::new(),
            string_builder_capacities: BTreeMap::new(),

            string_readers: BTreeMap::new(),
            scanners: BTreeMap::new(),
            io_stream_paths: BTreeMap::new(),
            io_writers: BTreeMap::new(),
            io_writer_charsets: BTreeMap::new(),
            io_paths,
            io_files,
            io_file_bytes,
            io_dirs,
            watch: Default::default(),
            annotations: BTreeMap::new(),
            next_annotation: 0,
            callback_infos: &mut callback_infos,
            invoke_hooks: None,
            anchor_hooks: None,
            modify_hooks: None,
            pending_skipped_new: Vec::new(),
            stream_plan: Default::default(),
            store_verdict: Default::default(),
            depth: 0,
            budget,
        };
        // `game_hook_callback_values` answers whether the callback is an
        // instance method: only those receive the game object as `this`.
        // The jar's own objects: what its entrypoint built and earlier
        // firings changed. Fields on game objects come from `state`.
        let resident = runtime.adopt_resident_heap();
        let outcome = (|| -> Result<(), NativeError> {
            for (entry, (is_instance, values)) in entries.iter().zip(checked) {
                let marker = callback_info_marker(&entry.target);
                // Every callback at one hook point shares the object Mixin would
                // hand the injection point, so a `cancel()` in one is what the
                // next one observes.
                runtime.open_callback_info(&marker)?;
                if let Some(actual) = &actual {
                    runtime.callback_infos.set_actual(&marker, actual.clone())?;
                }
                let before = hook_shapes::callback_info_snapshot(runtime.callback_infos, &marker)?;
                // No game object handed over: an object of the Mixin class
                // stands for it (its name is a `String`, #156 P13c; #156 P16).
                let callback_receiver = if is_instance {
                    Some(match receiver.clone() {
                        Some(receiver) => receiver,
                        None => runtime.new_object(&entry.callback.owner),
                    })
                } else {
                    None
                };
                // The first callback of a Mixin class on a game object runs the
                // class's constructor on it: `javac` put the Mixin's field
                // initialisers there, which real Mixin merges into the target's
                // constructors. A Mixin that extends a game class has no
                // no-argument constructor to run, and its fields start at Java's
                // defaults.
                if let Some(object) = callback_receiver
                    .as_ref()
                    .and_then(crate::runtime::game_object::text)
                    .filter(|object| object.starts_with(GAME_OBJECT_PREFIX))
                    && initialized.insert(format!("{}|{object}", entry.callback.owner))
                    && self
                        .classes
                        .get(&entry.callback.owner)
                        .is_some_and(|class| {
                            class.super_name == "java/lang/Object"
                                && class.method("<init>", "()V").is_some()
                        })
                {
                    runtime.invoke(
                        &entry.callback.owner,
                        "<init>",
                        "()V",
                        callback_receiver.clone(),
                        &[],
                    )?;
                }
                if entry.shape == GameHookShape::ModifyReturnValue {
                    // MixinExtras chains handlers: each gets the value as the
                    // ones before it left it, and its answer is the new one.
                    let mut handler_args = vec![runtime.callback_infos.current(&marker)?];
                    handler_args.extend(values);
                    let result = runtime.invoke(
                        &entry.callback.owner,
                        &entry.callback.name,
                        &entry.callback.descriptor,
                        callback_receiver,
                        &handler_args,
                    )?;
                    runtime.callback_infos.set_return(&marker, result.clone())?;
                    results.push(result);
                    continue;
                }
                let result = runtime.invoke(
                    &entry.callback.owner,
                    &entry.callback.name,
                    &entry.callback.descriptor,
                    callback_receiver,
                    &values,
                )?;
                results.push(result);
                let after = hook_shapes::callback_info_snapshot(runtime.callback_infos, &marker)?;
                if !entry.cancellable {
                    // Mixin's `CallbackInfo.cancel()` (and the `cancel()` a
                    // `setReturnValue` makes) throws on a callback that was not
                    // declared cancellable.
                    if after != before {
                        return Err(NativeError::Metadata(format!(
                            "uncaught exception org/spongepowered/asm/mixin/injection/callback/CancellationException: game hook callback {}#{}{} cancelled or set the result of {}#{}, which its @Inject does not declare cancellable",
                            entry.callback.owner,
                            entry.callback.name,
                            entry.callback.descriptor,
                            entry.target.owner,
                            entry.target.name
                        )));
                    }
                    continue;
                }
                // `CallbackInfoReturnable.setReturnValue` cancels as it sets.
                if after.0 || after.1.is_some() {
                    cancelled = true;
                    break;
                }
            }
            // A write to a `@Shadow` field, a game class's field or one a
            // Mixin inherits from the game class it extends is a write to the
            // game object's own field, which the host never sees: that
            // callback did not do what it meant to, so the firing fails
            // instead of keeping the write (#248's review).
            if let Some((key, shadow)) = runtime.fields.keys().find_map(|key| {
                let (object, field) = key.split_once('|')?;
                if !object.starts_with(GAME_OBJECT_PREFIX) {
                    return None;
                }
                let shadow = model.shadow_fields.contains(field);
                (shadow || model.game_field(field)).then_some((key, shadow))
            }) {
                let through = if shadow { " through @Shadow" } else { "" };
                return Err(NativeError::Metadata(format!(
                    "game hook callback writes the game's field {key}{through}, which the host does not apply"
                )));
            }
            if !resident
                && (runtime.fields.len() > MAX_GAME_HOOK_FIELDS
                    || runtime.arrays.len() > MAX_GAME_HOOK_ARRAYS)
            {
                return Err(NativeError::Metadata(format!(
                    "game hook callbacks keep more than {MAX_GAME_HOOK_FIELDS} fields or {MAX_GAME_HOOK_ARRAYS} arrays"
                )));
            }
            Ok(())
        })();
        let returned = outcome.as_ref().ok().map(|()| {
            hook_shapes::game_hook_returned(
                first_site,
                runtime.callback_infos,
                &callback_info_marker(&first.target),
                cancelled,
            )
        });
        if !resident {
            // A host without a resident heap: `state` keeps every object, and
            // a failed firing keeps nothing, as before the heap.
            outcome?;
            runtime.persist_class_state()?;
            runtime.persist_io_state()?;
            state.fields = std::mem::take(&mut runtime.fields);
            state.arrays = std::mem::take(&mut runtime.arrays);
        } else {
            // Game objects' fields stay with `state`, kept per object; the
            // jar's objects go back to the host, which keeps what the statics,
            // its registrations or those game-object fields still reach. A
            // failed firing's game-object writes are dropped.
            let (game, jar): (BTreeMap<_, _>, BTreeMap<_, _>) = std::mem::take(&mut runtime.fields)
                .into_iter()
                .partition(|(key, _)| key.starts_with(GAME_OBJECT_PREFIX));
            runtime.fields = jar;
            if outcome.is_ok() {
                state.fields = game;
            }
            state.arrays.clear();
            // Every value a game object's field holds, of any kind (an
            // `Optional` names its object in its key), through the heap's
            // one root text (#226 review).
            // What the firing hands the game (each callback's result and the
            // value a target returns) is a root of this store too; the game
            // retains what it keeps longer (`StdHost::retain`).
            let returned_value = returned
                .as_ref()
                .and_then(|returned| returned.as_ref().ok())
                .cloned()
                .flatten();
            let roots = state
                .fields
                .values()
                .chain(&results)
                .chain(returned_value.as_ref())
                .flat_map(crate::runtime::heap::value_texts)
                .map(str::to_owned)
                .collect();
            let persistence = runtime.persist_class_state();
            let io_persistence = runtime.persist_io_state();
            let heap = runtime.persist_resident_heap_with(roots);
            outcome?;
            persistence?;
            io_persistence?;
            heap?;
        }
        state.next_object = runtime.next_object;
        state.next_array = runtime.next_array;
        state.initialized = initialized;
        Ok(GameHookRun {
            hook: hook.to_owned(),
            ran: results.len(),
            cancelled,
            returned: returned.transpose()?.flatten(),
            results,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::{EventFacade, FacadePlayer};

    /// A hook callback's player is the object the event tier hands out for
    /// the same player (`events::player_marker`): one key, so `==` holds
    /// across tiers, and a respawn's new life is another object.
    #[test]
    fn a_hook_player_is_the_event_tiers_player() {
        let uuid = 0x0123_4567_89ab_cdef_0011_2233_4455_6677_u128;
        let facade = |life| EventFacade {
            players: vec![FacadePlayer {
                uuid,
                name: "Alex".into(),
                level: None,
                entity_id: Some(7),
                session: 2,
                life,
            }],
            ..EventFacade::default()
        };
        let event = crate::runtime::game_object::player_value(&facade(0), uuid);
        let (Value::Host(hook), Value::Host(event)) = (hook_value(uuid, 0), event) else {
            panic!("not handles")
        };
        assert_eq!(hook.same_object(&event), Some(true));
        let Value::Host(respawned) = hook_value(uuid, 1) else {
            panic!("not a handle")
        };
        assert_eq!(hook.same_object(&respawned), Some(false));
        let Value::Host(event_respawned) = crate::runtime::game_object::player_value(&facade(1), uuid)
        else {
            panic!("not a handle")
        };
        assert_eq!(respawned.same_object(&event_respawned), Some(true));
    }

    fn hook_value(uuid: u128, life: u32) -> Value {
        GameObject::player(uuid, "Alex")
            .with_incarnation(2, life)
            .with_entity_id(7)
            .value()
    }
}
